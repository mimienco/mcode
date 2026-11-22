//! One deadline covers stdin, child exit, and both captured streams.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use crate::git_binary::{GitError, MAX_STDOUT_BYTES};

#[derive(Debug)]
pub(crate) struct Captured {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

enum Message {
    Stdout(Result<Vec<u8>, String>),
    Stderr(Result<Vec<u8>, String>),
    Stdin(Result<(), String>),
    Overflow,
}

fn failure(detail: impl Into<String>) -> GitError {
    GitError::GitOperationFailed {
        command: "process capture".into(),
        stderr_redacted: crate::git_binary::redact_stderr(&detail.into()),
    }
}

pub(crate) fn capture(
    binary: &Path,
    cwd: &Path,
    args: &[&OsStr],
    stdin: Option<&[u8]>,
    timeout: Duration,
    limit: u64,
) -> Result<Captured, GitError> {
    let started = Instant::now();
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    // A parent Git hook must not redirect another repository's index or worktree.
    for variable in [
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
    ] {
        command.env_remove(variable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(windows)]
    let job = mcode_process::JobObject::create().map_err(|error| failure(error.to_string()))?;
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GitError::GitBinaryMissing
        } else {
            failure(error.to_string())
        }
    })?;
    #[cfg(windows)]
    if let Err(error) = job.assign(child.id()) {
        // A very short version probe may have already exited before assignment.
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Err(failure(error.to_string()));
        }
    }
    let (sender, receiver) = mpsc::channel();
    spawn_reader(child.stdout.take(), sender.clone(), true, limit);
    spawn_reader(child.stderr.take(), sender.clone(), false, limit);
    let mut input_done = stdin.is_none();
    if let Some(bytes) = stdin {
        let bytes = bytes.to_vec();
        let pipe = child.stdin.take();
        let sender = sender.clone();
        std::thread::spawn(move || {
            let result = pipe
                .ok_or_else(|| "stdin pipe is unavailable".to_owned())
                .and_then(|mut pipe| pipe.write_all(&bytes).map_err(|error| error.to_string()));
            let _ = sender.send(Message::Stdin(result));
        });
    }
    drop(sender);
    let mut stdout = None;
    let mut stderr = None;
    let mut status = None;
    let result = loop {
        if status.is_none() {
            match observe_exit(&mut child) {
                Ok(value) => status = value,
                Err(error) => break Err(failure(error.to_string())),
            }
        }
        if input_done
            && let (Some(status), Some(stdout), Some(stderr)) =
                (status.as_ref(), stdout.as_mut(), stderr.as_mut())
        {
            break Ok(Captured {
                status: *status,
                stdout: std::mem::take(stdout),
                stderr: std::mem::take(stderr),
            });
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break Err(GitError::GitTimeout);
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(10))) {
            Ok(Message::Stdout(Ok(bytes))) => stdout = Some(bytes),
            Ok(Message::Stderr(Ok(bytes))) => stderr = Some(bytes),
            Ok(Message::Stdin(Ok(()))) => input_done = true,
            Ok(
                Message::Stdout(Err(error))
                | Message::Stderr(Err(error))
                | Message::Stdin(Err(error)),
            ) => break Err(failure(error)),
            Ok(Message::Overflow) => {
                break Err(failure(format!("output exceeded {limit} byte cap")));
            }
            Err(mpsc::RecvTimeoutError::Disconnected)
                if stdout.is_none() || stderr.is_none() || !input_done =>
            {
                break Err(failure("capture worker ended before completing its stream"));
            }
            Err(_) => {
                if stdout.is_some() && stderr.is_some() && input_done && status.is_none() {
                    std::thread::sleep(remaining.min(Duration::from_millis(10)));
                }
            }
        }
    };
    if result.is_err() {
        #[cfg(windows)]
        {
            let _ = job.terminate();
        }
        terminate_child(&mut child, status.is_some());
    }
    // Pipe workers never block the caller after its deadline; owned-tree termination closes them.
    if cfg!(unix) || status.is_none() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    result
}

fn terminate_child(child: &mut Child, already_exited: bool) {
    #[cfg(unix)]
    {
        // waitid(WNOWAIT) retains the owned leader until group cleanup completes.
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    if !already_exited {
        let _ = child.kill();
    }
}

#[cfg(not(unix))]
fn observe_exit(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
    child.try_wait()
}

#[cfg(unix)]
fn observe_exit(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
    use std::os::unix::process::ExitStatusExt;
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    // Retain a zombie leader while inherited pipes may still belong to its children.
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id(),
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    if unsafe { info.si_pid() } == 0 {
        return Ok(None);
    }
    let status = unsafe { info.si_status() };
    let raw = match info.si_code {
        libc::CLD_EXITED => status << 8,
        libc::CLD_DUMPED => status | 0x80,
        _ => status,
    };
    Ok(Some(ExitStatus::from_raw(raw)))
}

fn spawn_reader<T: Read + Send + 'static>(
    pipe: Option<T>,
    sender: Sender<Message>,
    stdout: bool,
    limit: u64,
) {
    std::thread::spawn(move || {
        let result = (|| {
            let mut pipe = pipe.ok_or_else(|| "capture pipe is unavailable".to_owned())?;
            let mut bytes = Vec::new();
            let mut chunk = [0; 16 * 1024];
            loop {
                let count = match pipe.read(&mut chunk) {
                    Ok(count) => count,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.to_string()),
                };
                if count == 0 {
                    return Ok(bytes);
                }
                if bytes.len() as u64 + count as u64 > limit.min(MAX_STDOUT_BYTES) {
                    let _ = sender.send(Message::Overflow);
                    return Ok(bytes);
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
        })();
        let _ = sender.send(if stdout {
            Message::Stdout(result)
        } else {
            Message::Stderr(result)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_args(case: &str) -> Vec<std::ffi::OsString> {
        [
            "--ignored",
            "--exact",
            "process_capture::tests::capture_fixture_child",
            "--nocapture",
            "--skip",
            case,
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }

    #[test]
    #[ignore = "subprocess fixture; invoked only by capture regression tests"]
    fn capture_fixture_child() {
        let case = std::env::args().last().unwrap();
        match case.as_str() {
            "capture-chatty" => {
                std::io::stdout()
                    .write_all(&vec![b'x'; 256 * 1024])
                    .unwrap();
                let mut input = Vec::new();
                std::io::stdin().read_to_end(&mut input).unwrap();
                println!("READ_BYTES={}", input.len());
            }
            "capture-hold" => std::thread::sleep(Duration::from_secs(5)),
            "capture-descendant" => {
                let _child = Command::new(std::env::current_exe().unwrap())
                    .args(fixture_args("capture-hold"))
                    .spawn()
                    .unwrap();
            }
            _ => panic!("unknown capture fixture"),
        }
        std::process::exit(0);
    }

    fn run_fixture(
        case: &str,
        stdin: Option<&[u8]>,
        timeout: Duration,
        limit: u64,
    ) -> Result<Captured, GitError> {
        let args = fixture_args(case);
        let refs = args.iter().map(|arg| arg.as_os_str()).collect::<Vec<_>>();
        capture(
            &std::env::current_exe().unwrap(),
            &std::env::current_dir().unwrap(),
            &refs,
            stdin,
            timeout,
            limit,
        )
    }

    #[test]
    fn drains_output_before_and_during_large_stdin_writes() {
        let payload = vec![b'i'; 2 * 1024 * 1024];
        let output = run_fixture(
            "capture-chatty",
            Some(&payload),
            Duration::from_secs(5),
            MAX_STDOUT_BYTES,
        )
        .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("READ_BYTES=2097152"));
    }

    #[test]
    fn deadline_includes_a_child_that_never_reads_stdin() {
        let started = Instant::now();
        let result = run_fixture(
            "capture-hold",
            Some(&vec![b'i'; 2 * 1024 * 1024]),
            Duration::from_millis(200),
            MAX_STDOUT_BYTES,
        );
        assert!(matches!(result, Err(GitError::GitTimeout)));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn deadline_also_covers_inherited_pipes_after_the_direct_child_exits() {
        let started = Instant::now();
        let result = run_fixture(
            "capture-descendant",
            None,
            Duration::from_millis(200),
            MAX_STDOUT_BYTES,
        );
        assert!(matches!(result, Err(GitError::GitTimeout)));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn oversized_stream_fails_instead_of_returning_partial_success() {
        let result = run_fixture("capture-chatty", None, Duration::from_secs(3), 1024);
        assert!(
            matches!(result, Err(GitError::GitOperationFailed { stderr_redacted, .. }) if stderr_redacted.contains("byte cap"))
        );
    }
}
