//! Descendant snapshots: the OS process table as observed right now.
//!
//! Windows walks the Toolhelp snapshot (windows-sys, already linked for the
//! Job Object). POSIX runs `ps -axo pid=,ppid=,pgid=` with a 32 MB output
//! cap, a 1 s timeout, and `LC_ALL=C` so parsers never see localized output.
//! A snapshot is advisory: it is only ever used to aim a kill at children
//! captured together with the parent's pgid.

/// Hard cap on captured `ps` output: 32 MB.
#[cfg_attr(not(unix), allow(dead_code))]
const PS_CAPTURE_CAP: usize = 32 * 1024 * 1024;

/// `ps` must answer within 1 s or the snapshot is abandoned.
#[cfg_attr(not(unix), allow(dead_code))]
const PS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// Returns the direct + transitive children of `pid` at snapshot time.
#[allow(unused_variables)]
pub fn list_descendants(pid: u32) -> Vec<u32> {
    #[cfg(windows)]
    {
        windows_descendants(pid)
    }
    #[cfg(not(windows))]
    {
        posix_descendants(pid)
    }
}

#[cfg(windows)]
fn windows_descendants(pid: u32) -> Vec<u32> {
    use std::collections::BTreeMap;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut rows: BTreeMap<u32, u32> = BTreeMap::new();
    unsafe {
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                rows.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }

    // BFS down the parent links.
    let mut descendants = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for (child_pid, parent_pid) in &rows {
            if *parent_pid == parent && *child_pid != pid {
                descendants.push(*child_pid);
                frontier.push(*child_pid);
            }
        }
    }
    descendants.sort_unstable();
    descendants.dedup();
    descendants
}

#[cfg(not(windows))]
fn posix_descendants(pid: u32) -> Vec<u32> {
    use std::collections::BTreeMap;
    let mut command = std::process::Command::new("ps");
    command
        .args(["-axo", "pid=,ppid=,pgid="])
        .env("LC_ALL", "C");
    let Some(buf) = capture_posix_output(command, PS_TIMEOUT, PS_CAPTURE_CAP) else {
        return Vec::new();
    };

    let text = String::from_utf8_lossy(&buf);
    let mut parent_of: BTreeMap<u32, u32> = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        if let (Ok(child_pid), Ok(parent_pid)) =
            (fields[0].parse::<u32>(), fields[1].parse::<u32>())
        {
            parent_of.insert(child_pid, parent_pid);
        }
    }

    let mut descendants = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for (child_pid, parent_pid) in &parent_of {
            if *parent_pid == parent && *child_pid != pid {
                descendants.push(*child_pid);
                frontier.push(*child_pid);
            }
        }
    }
    descendants.sort_unstable();
    descendants.dedup();
    descendants
}

#[cfg(unix)]
fn capture_posix_output(
    mut command: std::process::Command,
    timeout: std::time::Duration,
    cap: usize,
) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let mut child = command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let pid = child.id() as libc::pid_t;
    let captured = (|| {
        let mut stdout = child.stdout.take()?;
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return None;
        }
        let started = Instant::now();
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            if started.elapsed() >= timeout {
                return None;
            }
            match stdout.read(&mut chunk) {
                Ok(0) => return Some(bytes),
                Ok(count) => {
                    if count > cap.saturating_sub(bytes.len()) {
                        return None;
                    }
                    bytes.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return None,
            }
        }
    })();
    // Keep the leader unreaped until the owned group has been terminated.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    captured
}

#[cfg(all(test, unix))]
mod bounded_capture_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn shell(script: &str) -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn silent_ps_wrapper_cannot_hold_the_snapshot_past_its_deadline() {
        let started = Instant::now();
        let captured =
            capture_posix_output(shell("sleep 30 & wait"), Duration::from_millis(100), 1024);
        assert!(captured.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn inherited_stdout_after_wrapper_exit_still_respects_deadline() {
        let started = Instant::now();
        let captured =
            capture_posix_output(shell("sleep 30 & exit 0"), Duration::from_millis(100), 1024);
        assert!(captured.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn successful_snapshot_is_complete_and_oversize_output_is_rejected() {
        let expected = b"1 0 1\n2 1 1\n";
        let captured = capture_posix_output(
            shell("printf '1 0 1\n2 1 1\n'"),
            Duration::from_secs(1),
            expected.len(),
        )
        .unwrap();
        assert_eq!(captured, expected);
        assert!(capture_posix_output(shell("printf '12345'"), Duration::from_secs(1), 4).is_none());
    }
}
