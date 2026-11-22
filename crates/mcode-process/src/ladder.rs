//! The stop ladder rungs: graceful signal, bounded wait, forced kill — with
//! a recycled-pid guard before any group-wide signal.

use std::process::ExitStatus;
#[cfg(windows)]
use std::process::Stdio;

use crate::handle::{ExitObservation, ProcessHandle};

/// Normalizes an ExitStatus into the shared observation shape.
pub fn observation_from_status(status: ExitStatus) -> ExitObservation {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitObservation {
            exit_code: status.code().unwrap_or(-1),
            signal: status.signal(),
        }
    }
    #[cfg(not(unix))]
    {
        ExitObservation {
            exit_code: status.code().unwrap_or(-1),
            signal: None,
        }
    }
}

/// Rung 1: ask politely. Windows `taskkill /pid <pid>` (window-close message,
/// no /F); POSIX SIGTERM to the direct pid.
#[allow(unused_variables)]
pub fn graceful_kill(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("taskkill")
            .creation_flags(0x08000000)
            .arg("/pid")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        // SAFETY: kill(2) with a signal constant; pid came from a live child.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 }
    }
}

/// Rung 2: forced kill. Windows: TerminateJobObject (whole tree) with
/// `taskkill /F /T /pid` as fallback. POSIX: SIGKILL to the spawn-time
/// process group after verifying pgid still matches (recycled-pid guard).
#[allow(unused_variables)]
pub fn force_kill(
    pid: u32,
    job: Option<&crate::job::JobObject>,
    pgid: Option<crate::Pgid>,
    handle: ProcessHandle,
) {
    let _ = handle;
    #[cfg(windows)]
    {
        if let Some(job) = job {
            let _ = job.terminate();
        }
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .creation_flags(0x08000000)
            .args(["/F", "/T", "/pid", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = job;
        if let Some(pgid) = pgid {
            // Recycled-pid guard: only group-kill when the recorded spawn-time
            // group still matches what the OS reports now.
            if group_matches(pid, pgid) {
                // SAFETY: kill(2) to a negative pid targets the group.
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
            } else {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGKILL);
                }
            }
        } else {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }
}

/// Verify group ownership with the OS on Linux and macOS before signaling the group.
#[cfg(unix)]
fn group_matches(pid: u32, expected_pgid: libc::pid_t) -> bool {
    if pid == 0 || expected_pgid <= 0 {
        return false;
    }
    unsafe { libc::getpgid(pid as libc::pid_t) == expected_pgid }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn group_ownership_uses_the_platform_process_table() {
        let pid = std::process::id();
        let pgid = unsafe { libc::getpgid(pid as libc::pid_t) };
        assert!(pgid > 0);
        assert!(group_matches(pid, pgid));
        assert!(!group_matches(pid, 0));
        assert!(!group_matches(0, pgid));
        assert!(!group_matches(pid, pgid.saturating_add(1)));
    }
}
