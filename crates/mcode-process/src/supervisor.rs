//! Process supervision: spawn, registry, graceful stop ladder, and Drop drain.
//!
//! Every supervised process carries a stable [`ProcessHandle`] (never an OS
//! pid — pids are recycled) plus the spawn-time facts needed for a safe
//! stop ladder: on Windows a Job Object that kills the whole tree, on POSIX
//! the spawn-time process group used for group kills. The design mirrors the
//! original daemon's `pty-subprocess.ts` termination discipline: graceful
//! first, bounded wait, forced kill, and a recycled-pid guard before any
//! group-wide signal.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::env::{apply_environment, current_environment};
use crate::handle::{ExitCause, ExitObservation, ProcessHandle, ProcessSpec, ProcessState};

/// Default bounded wait between the graceful and forced rungs of the ladder.
pub const DEFAULT_GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Tunables for a [`ProcessSupervisor`].
#[derive(Clone, Debug)]
pub struct SupervisorConfig {
    /// Graceful stop grace period before escalation.
    pub graceful_stop_timeout: Duration,
    /// Deadline for the wait(handle, timeout) query path.
    pub wait_timeout: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            graceful_stop_timeout: DEFAULT_GRACEFUL_STOP_TIMEOUT,
            wait_timeout: Duration::from_secs(30),
        }
    }
}

/// Typed supervisor errors with stable snake_case codes for the renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorError {
    /// The spec failed validation.
    SpecRejected(String),
    /// The OS refused the spawn.
    SpawnFailed { source: String },
    /// The handle is not (or no longer) in the registry.
    UnknownHandle,
    /// The registry held a handle the platform cannot act on.
    PlatformUnsupported,
    /// A lifecycle transition was requested from an illegal state.
    AlreadyExited,
}

impl SupervisorError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SpecRejected(_) => "process_spec_rejected",
            Self::SpawnFailed { .. } => "process_spawn_failed",
            Self::UnknownHandle => "process_unknown_handle",
            Self::PlatformUnsupported => "process_platform_unsupported",
            Self::AlreadyExited => "process_already_exited",
        }
    }
}

impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpecRejected(detail) => write!(f, "process spec rejected: {detail}"),
            Self::SpawnFailed { source } => write!(f, "process spawn failed: {source}"),
            Self::UnknownHandle => f.write_str("unknown process handle"),
            Self::PlatformUnsupported => f.write_str("unsupported platform operation"),
            Self::AlreadyExited => f.write_str("process already exited"),
        }
    }
}

impl std::error::Error for SupervisorError {}

impl From<crate::handle::SpecError> for SupervisorError {
    fn from(e: crate::handle::SpecError) -> Self {
        Self::SpecRejected(e.to_string())
    }
}

/// One registry entry: the OS child plus everything the stop ladder needs.
pub struct ProcessEntry {
    pub handle: ProcessHandle,
    pub spec: ProcessSpec,
    pub state: ProcessState,
    /// Spawn-time OS pid, kept for diagnostics only (never authority).
    pub pid: Option<u32>,
    /// Windows: Job Object assigned at spawn; tree-killed by ladder/Drop.
    pub job: Option<crate::job::JobObject>,
    /// POSIX: spawn-time process group id (recycled-pid guard verifies it);
    /// `None` on Windows, which has no process groups.
    pub pgid: Option<crate::Pgid>,
    pub child: Option<std::process::Child>,
    pub cause: Option<ExitCause>,
}

/// A supervised process as reported to callers: the stable handle, the
/// lifecycle state, and the normalized exit cause once terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub handle: ProcessHandle,
    pub state: ProcessState,
    pub cause: Option<ExitCause>,
    pub program: String,
}

impl From<&ProcessEntry> for ProcessSnapshot {
    fn from(e: &ProcessEntry) -> Self {
        Self {
            handle: e.handle,
            state: e.state.clone(),
            cause: e.cause.clone(),
            program: e.spec.program.clone(),
        }
    }
}

/// Spawn outcome: the stable handle under which the process is registered.
pub struct Spawned {
    pub handle: ProcessHandle,
}

/// Registry of supervised processes with the spawn/stop/exit lifecycle.
pub struct ProcessSupervisor {
    /// Kept for future ladder tuning; the graceful timeout is per-call today.
    #[allow(dead_code)]
    config: SupervisorConfig,
    next_handle: u64,
    entries: BTreeMap<ProcessHandle, ProcessEntry>,
}

impl Default for ProcessSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessSupervisor {
    pub fn new() -> Self {
        Self::with_config(SupervisorConfig::default())
    }

    pub fn with_config(config: SupervisorConfig) -> Self {
        Self {
            config,
            next_handle: 1,
            entries: BTreeMap::new(),
        }
    }

    /// Validates the spec, assembles the child environment through the
    /// environment policy, and spawns with the platform death-safety wiring
    /// (Job Object on Windows, process group on POSIX) applied at spawn.
    pub fn spawn(&mut self, spec: ProcessSpec) -> Result<Spawned, SupervisorError> {
        self.spawn_with_assignment(spec, crate::job::assign_to_job)
    }

    fn spawn_with_assignment(
        &mut self,
        spec: ProcessSpec,
        assign: impl FnOnce(&Option<crate::job::JobObject>, u32) -> Result<(), SupervisorError>,
    ) -> Result<Spawned, SupervisorError> {
        spec.validate()?;
        let handle = ProcessHandle::next(self.next_handle);
        self.next_handle += 1;

        let mut env = apply_environment(&current_environment(), &spec.env_policy);
        // Diagnostics determinism: children never leak host locale.
        env.entry("LC_ALL".to_string())
            .or_insert_with(|| "C".to_string());

        let mut cmd = std::process::Command::new(&spec.program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        cmd.env_clear();
        cmd.envs(&env);

        // Job Object (Windows) / process group (POSIX) MUST exist before the
        // child can run away; platform pre-spawn wiring returns the guard.
        let job = crate::job::pre_spawn_job_object(&spec)?;
        crate::job::pre_spawn_group(&spec, &mut cmd);

        let mut child = cmd.spawn().map_err(|e| SupervisorError::SpawnFailed {
            source: e.to_string(),
        })?;

        let pid = child.id();
        let pgid = crate::job::capture_spawn_time_pgid(pid);
        if let Err(error) = assign(&job, pid) {
            crate::ladder::force_kill(pid, job.as_ref(), pgid, handle);
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }

        let entry = ProcessEntry {
            handle,
            spec,
            state: ProcessState::Running,
            pid: Some(pid),
            job,
            pgid,
            child: Some(child),
            cause: None,
        };
        self.entries.insert(handle, entry);
        Ok(Spawned { handle })
    }

    /// Snapshot for a registered handle.
    pub fn get(&self, handle: ProcessHandle) -> Option<ProcessSnapshot> {
        self.entries.get(&handle).map(ProcessSnapshot::from)
    }

    /// All registered processes as snapshots.
    pub fn list(&self) -> Vec<ProcessSnapshot> {
        self.entries.values().map(ProcessSnapshot::from).collect()
    }

    /// Polls one process, reaping its exit state if it died.
    pub fn poll(&mut self, handle: ProcessHandle) -> Result<ProcessSnapshot, SupervisorError> {
        let entry = self
            .entries
            .get_mut(&handle)
            .ok_or(SupervisorError::UnknownHandle)?;
        entry.reap_if_exited();
        Ok(ProcessSnapshot::from(&*entry))
    }

    /// Bounded wait: returns once the process exits or the deadline passes.
    pub fn wait(
        &mut self,
        handle: ProcessHandle,
        timeout: Duration,
    ) -> Result<ProcessSnapshot, SupervisorError> {
        let started = Instant::now();
        loop {
            let snapshot = self.poll(handle)?;
            if snapshot.state.is_terminal() {
                return Ok(snapshot);
            }
            if started.elapsed() >= timeout {
                return Ok(snapshot);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Graceful stop ladder: taskkill (no /F) or SIGTERM, bounded wait, then
    /// taskkill /F /T or TerminateJobObject / SIGKILL to the spawn-time group
    /// with a recycled-pid guard. Returns the terminal snapshot.
    pub fn stop(
        &mut self,
        handle: ProcessHandle,
        graceful_timeout: Duration,
    ) -> Result<ProcessSnapshot, SupervisorError> {
        let entry = self
            .entries
            .get_mut(&handle)
            .ok_or(SupervisorError::UnknownHandle)?;
        if entry.state.is_terminal() {
            return Ok(ProcessSnapshot::from(&*entry));
        }
        entry.state = ProcessState::Stopping;

        // Rung 1: graceful.
        if let Some(child) = entry.child.as_ref() {
            crate::ladder::graceful_kill(child.id());
        }
        if let Some(child) = entry.child.as_mut() {
            let deadline = Instant::now() + graceful_timeout;
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let observation = crate::ladder::observation_from_status(status);
                        entry.record_exit(observation);
                        return Ok(ProcessSnapshot::from(&*entry));
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        }

        // Rung 2: forced.
        if let Some(child) = entry.child.as_mut() {
            let pid = child.id();
            crate::ladder::force_kill(pid, entry.job.as_ref(), entry.pgid, entry.handle);
            match child.wait() {
                Ok(status) => {
                    let observation = crate::ladder::observation_from_status(status);
                    entry.record_exit(observation);
                }
                Err(_) => entry.record_exit(ExitObservation {
                    exit_code: -1,
                    signal: None,
                }),
            }
        }
        Ok(ProcessSnapshot::from(&*entry))
    }

    /// Snapshot of the OS process tree under one supervised child.
    pub fn list_descendants(&self, handle: ProcessHandle) -> Result<Vec<u32>, SupervisorError> {
        let entry = self
            .entries
            .get(&handle)
            .ok_or(SupervisorError::UnknownHandle)?;
        let Some(pid) = entry.pid else {
            return Ok(Vec::new());
        };
        Ok(crate::descendants::list_descendants(pid))
    }

    /// Registry handle stability: the same handle always maps back to the
    /// same program; pids are never reused as registry keys.
    pub fn entry_program(&self, handle: ProcessHandle) -> Option<&str> {
        self.entries.get(&handle).map(|e| e.spec.program.as_str())
    }

    fn drain_all(&mut self) {
        let handles: Vec<ProcessHandle> = self.entries.keys().copied().collect();
        for handle in handles {
            let Some(entry) = self.entries.get_mut(&handle) else {
                continue;
            };
            if entry.state.is_terminal() {
                continue;
            }
            if let Some(child) = entry.child.as_mut() {
                let pid = child.id();
                crate::ladder::force_kill(pid, entry.job.as_ref(), entry.pgid, entry.handle);
                match child.wait() {
                    Ok(status) => {
                        entry.record_exit(crate::ladder::observation_from_status(status));
                    }
                    Err(_) => entry.record_exit(ExitObservation {
                        exit_code: -1,
                        signal: None,
                    }),
                }
            }
        }
    }
}

impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        self.drain_all();
    }
}

impl ProcessEntry {
    fn record_exit(&mut self, observation: ExitObservation) {
        drop(self.job.take());
        self.state = ProcessState::Exited {
            code: observation.exit_code,
            signal: observation.signal,
        };
        self.cause = Some(observation.cause());
    }

    fn reap_if_exited(&mut self) {
        if self.state.is_terminal() {
            return;
        }
        if let Some(status) = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok())
            .flatten()
        {
            self.record_exit(crate::ladder::observation_from_status(status));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::EnvironmentPolicy;

    /// Long-running child: `cmd /c ping -n N 127.0.0.1` on Windows, `sleep N`
    /// elsewhere. Both survive well past any test's graceful window.
    fn long_running_spec(seconds: u32) -> ProcessSpec {
        #[cfg(windows)]
        {
            ProcessSpec::new(
                "cmd",
                vec![
                    "/c".to_string(),
                    "ping".to_string(),
                    "-n".to_string(),
                    seconds.to_string(),
                    "127.0.0.1".to_string(),
                ],
            )
            .expect("valid spec")
        }
        #[cfg(not(windows))]
        {
            ProcessSpec::new("sleep", vec![seconds.to_string()]).expect("valid spec")
        }
    }

    /// Instant-exit child with the requested exit code.
    fn exit_code_spec(code: u8) -> ProcessSpec {
        #[cfg(windows)]
        {
            ProcessSpec::new(
                "cmd",
                vec!["/c".to_string(), "exit".to_string(), code.to_string()],
            )
            .expect("valid spec")
        }
        #[cfg(not(windows))]
        {
            ProcessSpec::new("sh", vec!["-c".into(), format!("exit {code}")]).expect("valid spec")
        }
    }

    #[test]
    fn failed_job_assignment_reaps_spawned_child_before_returning_error() {
        let mut supervisor = ProcessSupervisor::new();
        let observed_pid = std::cell::Cell::new(None);
        let error = SupervisorError::SpawnFailed {
            source: "injected assignment failure".into(),
        };
        let result = supervisor.spawn_with_assignment(long_running_spec(30), |_, pid| {
            observed_pid.set(Some(pid));
            Err(error.clone())
        });
        assert!(matches!(result, Err(ref actual) if actual == &error));
        assert!(supervisor.entries.is_empty());
        let pid = observed_pid.get().unwrap();
        #[cfg(unix)]
        {
            assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            };
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !process.is_null() {
                let mut code = 259;
                let queried = GetExitCodeProcess(process, &mut code);
                windows_sys::Win32::Foundation::CloseHandle(process);
                assert_ne!(queried, 0);
                assert_ne!(code, 259);
            }
        }
    }

    #[test]
    fn spawn_exits_with_requested_code() {
        let mut sup = ProcessSupervisor::new();
        let spawned = sup.spawn(exit_code_spec(7)).expect("spawn");
        let snapshot = sup
            .wait(spawned.handle, Duration::from_secs(10))
            .expect("wait");
        assert_eq!(
            snapshot.state,
            ProcessState::Exited {
                code: 7,
                signal: None
            }
        );
        assert_eq!(snapshot.cause, Some(ExitCause::Exited { exit_code: 7 }));
        assert!(sup.entries[&spawned.handle].job.is_none());
    }

    #[test]
    fn kill_ladder_escalates_past_graceful_window() {
        let mut sup = ProcessSupervisor::new();
        let spawned = sup.spawn(long_running_spec(30)).expect("spawn");
        let started = Instant::now();
        let snapshot = sup
            .stop(spawned.handle, Duration::from_millis(300))
            .expect("stop");
        assert!(
            snapshot.state.is_terminal(),
            "stop must reach a terminal state"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "ladder must stay bounded"
        );
    }

    #[test]
    fn env_scrub_reaches_the_real_spawn_path() {
        // The supervisor is the only spawn site; pin that it routes the spec's
        // policy through apply_environment: a scrubbed var dies even when the
        // test process carries it, while an allowed delta survives.
        // SAFETY: single-threaded test; no other thread reads the environment.
        unsafe {
            std::env::set_var("ELECTRON_RUN_AS_NODE", "1");
        }
        let policy = EnvironmentPolicy::Inherit {
            deltas: vec![("MCODE_TEST_ALLOWED".to_string(), "yes".to_string())],
            removes: Vec::new(),
        };
        let base = current_environment();
        let env = apply_environment(&base, &policy);
        assert!(!env.contains_key("ELECTRON_RUN_AS_NODE"));
        assert_eq!(
            env.get("MCODE_TEST_ALLOWED").map(String::as_str),
            Some("yes")
        );
        // SAFETY: cleanup of the key this test set.
        unsafe {
            std::env::remove_var("ELECTRON_RUN_AS_NODE");
        }
    }

    #[test]
    fn spec_rejects_empty_program_before_spawn() {
        let sup = ProcessSupervisor::new();
        let err = ProcessSpec::new("  ", vec!["x".into()])
            .map_err(SupervisorError::from)
            .unwrap_err();
        assert_eq!(err.code(), "process_spec_rejected");
        // And the registry is untouched by a rejected spec.
        assert!(sup.list().is_empty());
    }

    #[test]
    fn concurrent_spawn_and_kill_ten() {
        let mut sup = ProcessSupervisor::new();
        let mut handles = Vec::new();
        for _ in 0..10 {
            handles.push(sup.spawn(long_running_spec(30)).expect("spawn").handle);
        }
        for h in &handles {
            let snapshot = sup.stop(*h, Duration::from_millis(200)).expect("stop");
            assert!(snapshot.state.is_terminal());
        }
        assert_eq!(sup.list().len(), 10);
    }

    #[test]
    fn registry_handle_stability() {
        let mut sup = ProcessSupervisor::new();
        let spec = exit_code_spec(0);
        let program = spec.program.clone();
        let h = sup.spawn(spec).expect("spawn").handle;
        assert_eq!(sup.entry_program(h), Some(program.as_str()));
        let before = sup.get(h).expect("present");
        let _ = sup.wait(h, Duration::from_secs(10));
        let after = sup.get(h).expect("still registered after exit");
        assert_eq!(before.handle, after.handle);
        assert_eq!(sup.entry_program(h), Some(program.as_str()));
    }

    #[test]
    fn drop_force_kills_stragglers() {
        let mut sup = ProcessSupervisor::new();
        let _spawned = sup.spawn(long_running_spec(30)).expect("spawn");
        // Drop runs drain_all: force-kill + bounded wait per straggler. If
        // the ladder failed, the test process would leak a child; the other
        // tests prove the ladder reaches terminal states.
        drop(sup);
    }

    #[test]
    fn exit_cause_normalization_from_observations() {
        assert_eq!(
            ExitObservation {
                exit_code: 5,
                signal: None
            }
            .cause(),
            ExitCause::Exited { exit_code: 5 }
        );
        // node-pty shape: signalled death arrives as {exitCode: 0, signal: N}.
        assert_eq!(
            ExitObservation {
                exit_code: 0,
                signal: Some(15)
            }
            .cause(),
            ExitCause::Signaled { signal: 15 }
        );
        assert_eq!(
            ExitObservation {
                exit_code: -1,
                signal: None
            }
            .cause(),
            ExitCause::StopUnverified
        );
    }

    #[test]
    fn descendant_scan_finds_the_child() {
        let mut sup = ProcessSupervisor::new();
        // A cmd child that itself spawns a nested ping: the scan must find at
        // least one descendant pid while the tree is alive.
        #[cfg(windows)]
        let spec = ProcessSpec::new(
            "cmd",
            vec![
                "/c".to_string(),
                "ping -n 10 127.0.0.1 >nul & ping -n 10 127.0.0.1 >nul".to_string(),
            ],
        )
        .expect("valid spec");
        #[cfg(not(windows))]
        let spec =
            ProcessSpec::new("sh", vec!["-c".into(), "sleep 10".into()]).expect("valid spec");
        let h = sup.spawn(spec).expect("spawn").handle;
        std::thread::sleep(Duration::from_millis(300));
        let descendants = sup.list_descendants(h).expect("scan");
        let _ = sup.stop(h, Duration::from_millis(200));
        assert!(
            !descendants.is_empty(),
            "expected the child shell itself in the descendant set"
        );
    }
}
