//! Process identity, spawn specifications, and lifecycle states.
//!
//! A [`ProcessHandle`] is a stable, supervisor-allocated id — never an OS pid.
//! OS pids are recycled by every platform; handles are not. Everything that
//! crosses an IPC boundary or outlives a spawn call carries the handle.
//!
//! A [`ProcessSpec`] describes an argv vector plus environment policy. Shell
//! strings are intentionally unrepresentable: there is no `command: String`
//! field and no way to smuggle one in, mirroring the original daemon where a
//! startup command is a separate, explicit concern.

use serde::{Deserialize, Serialize};

/// Stable supervisor-allocated process id. Distinct from any OS pid.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProcessHandle(u64);

impl ProcessHandle {
    pub fn raw(self) -> u64 {
        self.0
    }

    /// Supervisor-internal constructor; handles are minted only by the
    /// registry, never parsed from user input.
    pub(crate) fn next(raw: u64) -> Self {
        Self(raw)
    }
}

impl std::fmt::Display for ProcessHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "process-{}", self.0)
    }
}

/// How the child's environment is assembled from the supervisor's own env.
///
/// The scrub list is applied in *every* policy — even [`EnvironmentPolicy::Explicit`]
/// never re-introduces a scrubbed key, because these keys leak host or Electron
/// identity into user processes (see `EnvironmentPolicy::scrub_keys`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentPolicy {
    /// Inherit the parent environment, then apply the allowlisted deltas.
    Inherit {
        /// Keys added or replaced on top of the inherited environment.
        deltas: Vec<(String, String)>,
        /// Keys removed from the inherited environment before deltas apply.
        removes: Vec<String>,
    },
    /// An explicit environment replaces the inherited one entirely
    /// (PATH and SystemRoot-style essentials are the caller's responsibility).
    Explicit {
        map: Vec<(String, String)>,
        /// Keys removed from the explicit map.
        removes: Vec<String>,
    },
}

impl Default for EnvironmentPolicy {
    fn default() -> Self {
        Self::Inherit {
            deltas: Vec::new(),
            removes: Vec::new(),
        }
    }
}

impl EnvironmentPolicy {
    /// Keys that must never reach a child process.
    ///
    /// Grounded in the original daemon scrubbing (`pty-subprocess.ts:88-93,190,
    /// 244-260,655-690`): Electron's run-as-node flag turns nested Electron
    /// commands into plain Node; `NODE_OPTIONS` smuggles code into every Node
    /// child; the pane identity keys would let a spawned shell impersonate a
    /// different pane's identity; the AppImage runtime keys make host commands
    /// believe they were invoked as MCode.
    pub fn scrub_keys() -> &'static [&'static str] {
        &[
            "ELECTRON_RUN_AS_NODE",
            "NODE_OPTIONS",
            "MCODE_PANE_KEY",
            "MCODE_TAB_ID",
            "MCODE_WORKTREE_ID",
            "MCODE_AGENT_LAUNCH_TOKEN",
            "MCODE_AGENT_HOOK_ENDPOINT",
            "MCODE_INTERNAL_TERMINAL_GIT_CREDENTIAL_GUARD_POLICY",
            "APPIMAGE",
            "APPDIR",
            "ARGV0",
            "OWD",
            "APPIMAGE_LIBRARY_PATH",
        ]
    }

    pub fn is_explicit(&self) -> bool {
        matches!(self, Self::Explicit { .. })
    }
}

/// A validated spawn request: argv vector, working directory, environment
/// policy. No shell string is ever accepted — `program` plus `args` only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env_policy: EnvironmentPolicy,
}

/// Why a spec was rejected before any OS resource was created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecError {
    EmptyProgram,
    EmptyArgs,
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyProgram => f.write_str("process spec program is empty"),
            Self::EmptyArgs => f.write_str("process spec args are empty"),
        }
    }
}

impl std::error::Error for SpecError {}

impl ProcessSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Result<Self, SpecError> {
        let spec = Self {
            program: program.into(),
            args,
            cwd: None,
            env_policy: EnvironmentPolicy::default(),
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_env_policy(mut self, env_policy: EnvironmentPolicy) -> Self {
        self.env_policy = env_policy;
        self
    }

    /// Rejects an empty program and an empty argv vector before spawn.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.program.trim().is_empty() {
            return Err(SpecError::EmptyProgram);
        }
        if self.args.is_empty() {
            return Err(SpecError::EmptyArgs);
        }
        Ok(())
    }
}

/// Lifecycle state of one supervised process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ProcessState {
    /// Spawn issued, exit not yet reaped.
    Starting,
    /// Spawn completed, process confirmed alive.
    Running,
    /// A stop ladder is in progress against this process.
    Stopping,
    /// Terminal state. `signal` is set when the OS reported a signal death.
    Exited { code: i32, signal: Option<i32> },
}

impl ProcessState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Exited { .. })
    }
}

/// Structured exit cause, normalized from what the OS actually observed.
///
/// Mirrors the original `TerminalExitCause` discipline (`terminal-exit-cause.ts`):
/// node-pty-style hosts report a signalled death as `{exitCode: 0, signal: N}`,
/// and a bare number forces every reader to guess. The cause is built once,
/// where the facts are in hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExitCause {
    /// The process ended on its own; the host vouched for the status.
    Exited { exit_code: i32 },
    /// The host reported a signal; the process did not choose to stop.
    Signaled { signal: i32 },
    /// A stop was issued and no exit was ever observed.
    StopUnverified,
}

/// One normalized observation of a child's death: `{exitCode, signal?}`.
///
/// `None` signal with a negative exit code means "asked to stop, never saw it
/// die" — that is an absence of evidence, not an exit status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitObservation {
    pub exit_code: i32,
    pub signal: Option<i32>,
}

impl ExitObservation {
    /// Builds a cause from evidence actually held.
    ///
    /// A positive signal always wins over the exit code (node-pty reports 0 for
    /// a signalled death). A negative code with no signal normalizes to
    /// [`ExitCause::StopUnverified`] rather than a fabricated status.
    pub fn cause(self) -> ExitCause {
        if let Some(signal) = self.signal.filter(|s| *s > 0) {
            return ExitCause::Signaled { signal };
        }
        if self.exit_code < 0 {
            return ExitCause::StopUnverified;
        }
        ExitCause::Exited {
            exit_code: self.exit_code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_rejects_empty_program() {
        let err = ProcessSpec::new("   ", vec!["x".into()]).unwrap_err();
        assert_eq!(err, SpecError::EmptyProgram);
    }

    #[test]
    fn spec_rejects_empty_args() {
        let err = ProcessSpec::new("cmd", Vec::new()).unwrap_err();
        assert_eq!(err, SpecError::EmptyArgs);
    }

    #[test]
    fn spec_accepts_valid_program_and_args() {
        let spec = ProcessSpec::new("cmd", vec!["/c".into(), "echo".into()]).unwrap();
        assert_eq!(spec.program, "cmd");
        assert!(spec.cwd.is_none());
    }

    #[test]
    fn scrub_list_contains_required_keys() {
        let scrub = EnvironmentPolicy::scrub_keys();
        for key in [
            "ELECTRON_RUN_AS_NODE",
            "NODE_OPTIONS",
            "MCODE_PANE_KEY",
            "MCODE_TAB_ID",
            "MCODE_WORKTREE_ID",
            "MCODE_AGENT_LAUNCH_TOKEN",
        ] {
            assert!(scrub.contains(&key), "scrub list missing {key}");
        }
    }

    #[test]
    fn exit_observation_signal_wins_over_zero_code() {
        // node-pty reports a signalled death as {exitCode: 0, signal: N}.
        let cause = ExitObservation {
            exit_code: 0,
            signal: Some(15),
        }
        .cause();
        assert_eq!(cause, ExitCause::Signaled { signal: 15 });
    }

    #[test]
    fn exit_observation_negative_code_normalizes_to_stop_unverified() {
        let cause = ExitObservation {
            exit_code: -1,
            signal: None,
        }
        .cause();
        assert_eq!(cause, ExitCause::StopUnverified);
    }

    #[test]
    fn exit_observation_clean_code_normalizes_to_exited() {
        let cause = ExitObservation {
            exit_code: 3,
            signal: None,
        }
        .cause();
        assert_eq!(cause, ExitCause::Exited { exit_code: 3 });
    }

    #[test]
    fn process_state_terminality() {
        assert!(!ProcessState::Running.is_terminal());
        assert!(
            ProcessState::Exited {
                code: 0,
                signal: None
            }
            .is_terminal()
        );
    }

    #[test]
    fn handle_display_is_stable_and_ordered() {
        let a = ProcessHandle(1);
        let b = ProcessHandle(2);
        assert_eq!(a.to_string(), "process-1");
        assert!(a < b);
        assert_eq!(a.raw(), 1);
    }
}
