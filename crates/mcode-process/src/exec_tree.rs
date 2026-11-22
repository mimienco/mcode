//! Tree ownership for an asynchronously waited child, without leaking OS handles to its caller.

pub struct ExecTree {
    pid: Option<u32>,
    #[cfg(windows)]
    job: crate::JobObject,
    #[cfg(windows)]
    assigned: bool,
}

// SAFETY: the job HANDLE is exclusively owned and Windows permits using/closing it on any thread.
#[cfg(windows)]
unsafe impl Send for ExecTree {}

impl ExecTree {
    pub fn prepare(command: &mut std::process::Command) -> Result<Self, crate::SupervisorError> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        Ok(Self {
            pid: None,
            #[cfg(windows)]
            job: crate::JobObject::create()?,
            #[cfg(windows)]
            assigned: false,
        })
    }

    /// Attach immediately after spawn; failure leaves the guard armed to kill its own child.
    pub fn attach(&mut self, pid: u32) -> Result<(), crate::SupervisorError> {
        self.pid = Some(pid);
        #[cfg(windows)]
        {
            self.job.assign(pid)?;
            self.assigned = true;
        }
        Ok(())
    }

    /// The caller must retain the unreaped child until all inherited output handles close.
    pub fn terminate(&mut self) {
        let Some(pid) = self.pid.take() else {
            return;
        };
        #[cfg(unix)]
        {
            // SAFETY: the owned, unreaped child reserves pid; prepare created this isolated group.
            unsafe {
                libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            if !self.assigned || !self.job.terminate() {
                use std::os::windows::process::CommandExt;
                let _ = std::process::Command::new("taskkill")
                    .creation_flags(0x0800_0000)
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }

    pub fn disarm(&mut self) {
        self.pid = None;
    }
}

impl Drop for ExecTree {
    fn drop(&mut self) {
        self.terminate();
    }
}
