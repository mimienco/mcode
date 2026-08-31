//! Spawn-time death-safety wiring: Job Object on Windows, process group on
//! POSIX. These rungs exist so the stop ladder can always kill the whole
//! tree even if the direct child already died or detached.

#[cfg(windows)]
use crate::supervisor::SupervisorError;

/// Windows Job Object guard: closing the handle kills everything assigned
/// (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE), so Drop is itself a safety rung.
#[cfg(windows)]
pub struct JobObject {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
impl JobObject {
    pub fn create() -> Result<Self, SupervisorError> {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(SupervisorError::PlatformUnsupported);
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                windows_sys::Win32::Foundation::CloseHandle(handle);
                return Err(SupervisorError::PlatformUnsupported);
            }
            Ok(Self { handle })
        }
    }

    pub fn assign(&self, pid: u32) -> Result<(), SupervisorError> {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };
        unsafe {
            // AssignProcessToJobObject needs a process handle with
            // PROCESS_SET_QUOTA | PROCESS_TERMINATE, not a pseudo pid handle.
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return Err(SupervisorError::SpawnFailed {
                    source: "OpenProcess for job assignment failed".to_string(),
                });
            }
            let assigned = AssignProcessToJobObject(self.handle, process);
            windows_sys::Win32::Foundation::CloseHandle(process);
            if assigned == 0 {
                return Err(SupervisorError::SpawnFailed {
                    source: "AssignProcessToJobObject failed".to_string(),
                });
            }
            Ok(())
        }
    }

    /// Kills every process in the job, then closes the handle.
    pub fn terminate(&self) -> bool {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        unsafe { TerminateJobObject(self.handle, 1) != 0 }
    }
}

/// Creates the Job Object before the child exists (Windows) or `None` where
/// the platform instead relies on process groups.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn pre_spawn_job_object(
    _spec: &crate::handle::ProcessSpec,
) -> Result<Option<JobObject>, crate::supervisor::SupervisorError> {
    #[cfg(windows)]
    {
        Ok(Some(JobObject::create()?))
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

/// POSIX-only pre-spawn wiring: makes the child its own process group leader
/// so group kills have a stable spawn-time target (pgid == pid).
#[allow(unused_variables)]
pub fn pre_spawn_group(_spec: &crate::handle::ProcessSpec, cmd: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }
    }
}

/// Captures the spawn-time pgid. On POSIX the child was made a group leader,
/// so pgid == pid; on Windows there is no group and this is `None`.
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn capture_spawn_time_pgid(pid: u32) -> Option<crate::Pgid> {
    #[cfg(unix)]
    {
        Some(pid as libc::pid_t)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Assigns an already-spawned pid into the job (Windows only).
#[allow(unused_variables)]
pub fn assign_to_job(
    job: &Option<JobObject>,
    pid: u32,
) -> Result<(), crate::supervisor::SupervisorError> {
    #[cfg(windows)]
    {
        if let Some(job) = job {
            job.assign(pid)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}
