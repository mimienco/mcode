//! Local PTY spawning through `portable-pty` (ConPTY on Windows, openpty on
//! POSIX) with typed spawn errors.

use mcode_process::EnvironmentPolicy;
use portable_pty::{CommandBuilder, PtySize as NativePtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Handle for one live PTY session. Distinct from any OS pid.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PtyHandle(pub u64);

/// Requested terminal size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// A validated PTY spawn request: argv vector, cwd, environment policy,
/// size. Like [`mcode_process::ProcessSpec`], no shell string is ever
/// accepted — the shell resolution (`shell.rs`) is a separate explicit step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtySpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env_policy: EnvironmentPolicy,
    pub size: PtySize,
    pub scrollback_rows: u32,
}

impl PtySpec {
    /// Validates before any OS resource is created.
    pub fn validate(&self) -> Result<(), PtyError> {
        if self.program.trim().is_empty() {
            return Err(PtyError::PtyInvalidArgument {
                detail: "program must not be empty".to_string(),
            });
        }
        if self.args.is_empty() {
            return Err(PtyError::PtyInvalidArgument {
                detail: "args must not be empty".to_string(),
            });
        }
        Ok(())
    }
}

/// Typed PTY-layer errors with stable snake_case codes for the renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PtyError {
    /// The platform PTY system refused the spawn.
    PtySpawnFailed { source: String },
    /// A pre-spawn argument was rejected.
    PtyInvalidArgument { detail: String },
    /// Input exceeded the 16 MiB cap.
    InputTooLarge,
    /// The session was already gone when the operation ran.
    SessionGone,
}

impl PtyError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PtySpawnFailed { .. } => "pty_spawn_failed",
            Self::PtyInvalidArgument { .. } => "pty_invalid_argument",
            Self::InputTooLarge => "pty_input_too_large",
            Self::SessionGone => "pty_session_gone",
        }
    }
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PtySpawnFailed { source } => write!(f, "pty spawn failed: {source}"),
            Self::PtyInvalidArgument { detail } => write!(f, "invalid pty argument: {detail}"),
            Self::InputTooLarge => {
                f.write_str("terminal input is too large for a safe terminal send")
            }
            Self::SessionGone => f.write_str("pty session is gone"),
        }
    }
}

impl std::error::Error for PtyError {}

/// Terminal input cap: 16 MiB (`terminal-input.ts:8`).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Builds the portable-pty command from the spec with the environment
/// policy applied: scrub list first, then deltas/explicit map — the same
/// discipline as the process supervisor's spawn path.
pub fn build_command(spec: &PtySpec) -> CommandBuilder {
    let base: BTreeMap<String, String> = if spec.env_policy.is_explicit() {
        BTreeMap::new()
    } else {
        std::env::vars().collect()
    };
    let env = mcode_process::apply_environment(&base, &spec.env_policy);

    let mut cmd = CommandBuilder::new(&spec.program);
    cmd.args(spec.args.iter());
    if let Some(cwd) = &spec.cwd {
        cmd.cwd(cwd);
    }
    // portable-pty's CommandBuilder inherits the parent env by default; with
    // an explicit policy we clear it so only the given map reaches the child.
    if spec.env_policy.is_explicit() {
        cmd.env_clear();
    }
    for (k, v) in &env {
        cmd.env(k, v);
    }
    cmd
}

/// Opens a pty pair for the spec's size.
pub fn open_pty(spec: &PtySpec) -> Result<portable_pty::PtyPair, PtyError> {
    let size = NativePtySize {
        rows: spec.size.rows,
        cols: spec.size.cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    native_pty_system()
        .openpty(size)
        .map_err(|e| PtyError::PtySpawnFailed {
            source: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_rejects_empty_program() {
        let spec = PtySpec {
            program: "  ".to_string(),
            args: vec!["x".to_string()],
            cwd: None,
            env_policy: EnvironmentPolicy::default(),
            size: PtySize::default(),
            scrollback_rows: 1000,
        };
        assert_eq!(spec.validate().unwrap_err().code(), "pty_invalid_argument");
    }

    #[test]
    fn spec_rejects_empty_args() {
        let spec = PtySpec {
            program: "cmd".to_string(),
            args: Vec::new(),
            cwd: None,
            env_policy: EnvironmentPolicy::default(),
            size: PtySize::default(),
            scrollback_rows: 1000,
        };
        assert_eq!(spec.validate().unwrap_err().code(), "pty_invalid_argument");
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            PtyError::PtySpawnFailed { source: "x".into() }.code(),
            "pty_spawn_failed"
        );
        assert_eq!(PtyError::InputTooLarge.code(), "pty_input_too_large");
        assert_eq!(
            PtyError::PtyInvalidArgument { detail: "x".into() }.code(),
            "pty_invalid_argument"
        );
    }

    #[test]
    fn pty_handle_is_opaque_and_orderable() {
        let a = PtyHandle(1);
        let b = PtyHandle(2);
        assert!(a < b);
        assert_eq!(a, PtyHandle(1));
    }

    #[test]
    fn build_command_applies_scrub_policy() {
        // SAFETY: test-only env mutation, single-threaded harness context.
        unsafe {
            std::env::set_var("MCODE_PTY_SCRUB_PROBE", "1");
        }
        let spec = PtySpec {
            program: "cmd".to_string(),
            args: vec!["/c".to_string()],
            cwd: None,
            env_policy: EnvironmentPolicy::Inherit {
                deltas: vec![("MCODE_PTY_ALLOWED".to_string(), "yes".to_string())],
                removes: Vec::new(),
            },
            size: PtySize::default(),
            scrollback_rows: 1000,
        };
        let cmd = build_command(&spec);
        let env: Vec<(String, String)> = cmd
            .iter_full_env_as_str()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(
            !env.iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("MCODE_PANE_KEY")),
            "scrubbed key must not reach the child command"
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "MCODE_PTY_ALLOWED" && v == "yes")
        );
        unsafe {
            std::env::remove_var("MCODE_PTY_SCRUB_PROBE");
        }
    }
}
