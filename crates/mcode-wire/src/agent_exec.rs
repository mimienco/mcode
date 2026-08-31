//! Non-interactive agent exec over the relay — port of
//! `src/relay/agent-exec-handler.ts`. Used by the AI commit-message/PR-field
//! generator to spawn agent CLIs with the staged diff on stdin and output
//! captured; distinct from pty.spawn (no terminal allocation, no escape
//! sequences, clean exit code).
//!
//! Semantics preserved: per-(cwd, operation) lanes so cancel targets only the
//! newest visible job; output capped at 4 MiB per stream (overflow kills the
//! tree); timeout clamp 1s–5min with tree-kill (CLIs trap SIGTERM; Windows
//! `.cmd` shims wrap the real process); structured ExecResult instead of
//! throwing.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// agent-exec-handler.ts:9.
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
/// agent-exec-handler.ts:10.
pub const MAX_TIMEOUT_MS: u64 = 5 * 60 * 1000;
/// agent-exec-handler.ts:11 — per-stream output cap; overflow kills the tree.
pub const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// agent-exec-handler.ts:92-101.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    /// `None` when the process was killed (timeout/cancel) or failed to spawn.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canceled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawn_error: Option<String>,
}

/// agent-exec-handler.ts:76-91 — validated request shape.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecParams {
    pub binary: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub stdin: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub env: Option<Vec<(String, String)>>,
    #[serde(default)]
    pub operation: Option<String>,
}

/// agent-exec-handler.ts:85 — one visible non-interactive job per cwd+operation.
pub fn lane_key(cwd: Option<&str>, operation: Option<&str>) -> String {
    format!(
        "{}\u{0}{}",
        cwd.unwrap_or_default(),
        operation.unwrap_or_default()
    )
}

fn clamp_timeout(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1_000, MAX_TIMEOUT_MS)
}

pub struct LaneRegistry {
    in_flight: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
}

impl Default for LaneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LaneRegistry {
    pub fn new() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// Registers the lane; canceling any prior job (agent-exec-handler.ts:223-231).
    pub fn register(&self, key: &str) -> tokio::sync::watch::Receiver<bool> {
        let (tx, rx) = tokio::sync::watch::channel(false);
        if let Some(prior) = self
            .in_flight
            .lock()
            .expect("lane lock")
            .insert(key.to_string(), tx)
        {
            let _ = prior.send(true);
        }
        rx
    }

    /// agent-exec-handler.ts:47-58 — cancel resolves `{ canceled: true }` only
    /// when a job is live.
    pub fn cancel(&self, key: &str) -> bool {
        match self.in_flight.lock().expect("lane lock").get(key) {
            Some(tx) => {
                let _ = tx.send(true);
                true
            }
            None => false,
        }
    }

    pub fn release(&self, key: &str) {
        self.in_flight.lock().expect("lane lock").remove(key);
    }
}

/// Runs one non-interactive exec. Invalid/empty binaries surface as
/// `spawn_error` (the TS handler resolves, never throws, for spawn issues).
pub async fn exec(
    registry: &LaneRegistry,
    params: &ExecParams,
    extra_canceled_rx: Option<tokio::sync::watch::Receiver<bool>>,
) -> ExecResult {
    if params.binary.trim().is_empty() {
        return ExecResult {
            spawn_error: Some("agent.execNonInteractive: binary is required".into()),
            ..ExecResult::default()
        };
    }
    let timeout = Duration::from_millis(clamp_timeout(params.timeout_ms));
    let lane = if params.cwd.is_some() {
        Some(lane_key(params.cwd.as_deref(), params.operation.as_deref()))
    } else {
        None
    };
    let cancel_rx = lane.as_ref().map(|key| registry.register(key));

    let mut command = Command::new(&params.binary);
    command
        .args(&params.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // windowsHide: true — no console window for headless CLIs.
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    if let Some(cwd) = &params.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in params.env.iter().flatten() {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(key) = &lane {
                registry.release(key);
            }
            return ExecResult {
                spawn_error: Some(error.to_string()),
                ..ExecResult::default()
            };
        }
    };

    // Stdin: staged diff or hook payload.
    if let Some(mut stdin) = child.stdin.take() {
        let payload = params.stdin.clone().unwrap_or_default();
        tokio::spawn(async move {
            let _ = stdin.write_all(payload.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }

    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    let mut canceled_by_lane = false;

    let result = tokio::select! {
        _ = &mut deadline => ExecResult {
            timed_out: true,
            ..ExecResult::default()
        },
        output = child.wait_with_output() => match output {
            Ok(output) => ExecResult {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                exit_code: output.status.code(),
                timed_out: false,
                canceled: None,
                spawn_error: None,
            },
            Err(error) => ExecResult {
                spawn_error: Some(error.to_string()),
                ..ExecResult::default()
            },
        },
    };

    // Lane cancel after completion resolves canceled=true (the TS handler
    // marks canceled when cancel raced the exit).
    if let Some(rx) = cancel_rx {
        canceled_by_lane = *rx.borrow();
    }
    if let Some(rx) = extra_canceled_rx {
        canceled_by_lane = canceled_by_lane || *rx.borrow();
    }
    if let Some(key) = &lane {
        registry.release(key);
    }

    ExecResult {
        canceled: canceled_by_lane.then_some(true),
        ..result
    }
}

/// agent-exec-handler.ts:47-58 — cancel a live lane; `false` when none.
pub fn cancel(registry: &LaneRegistry, cwd: Option<&str>, operation: Option<&str>) -> bool {
    registry.cancel(&lane_key(cwd, operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn echo_binary(text: &str) -> (String, Vec<String>) {
        ("cmd".into(), vec!["/c".into(), "echo".into(), text.into()])
    }
    #[cfg(not(windows))]
    fn echo_binary(text: &str) -> (String, Vec<String>) {
        ("echo".into(), vec![text.into()])
    }

    #[tokio::test]
    async fn captures_stdout_and_exit_code() {
        let registry = LaneRegistry::new();
        let (binary, args) = echo_binary("agent-exec-ok");
        let result = exec(
            &registry,
            &ExecParams {
                binary,
                args,
                cwd: None,
                stdin: None,
                timeout_ms: None,
                env: None,
                operation: None,
            },
            None,
        )
        .await;
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("agent-exec-ok"), "{result:?}");
        assert!(!result.timed_out);
        assert_eq!(result.canceled, None);
    }

    #[tokio::test]
    async fn empty_binary_resolves_with_spawn_error() {
        let registry = LaneRegistry::new();
        let result = exec(
            &registry,
            &ExecParams {
                binary: "  ".into(),
                args: vec![],
                cwd: None,
                stdin: None,
                timeout_ms: None,
                env: None,
                operation: None,
            },
            None,
        )
        .await;
        assert_eq!(
            result.spawn_error.as_deref(),
            Some("agent.execNonInteractive: binary is required")
        );
    }

    #[tokio::test]
    async fn timeout_clamps_and_kills() {
        let registry = LaneRegistry::new();
        let result = exec(
            &registry,
            &ExecParams {
                #[cfg(windows)]
                binary: "ping".into(),
                #[cfg(windows)]
                args: vec!["-n".into(), "30".into(), "127.0.0.1".into()],
                #[cfg(not(windows))]
                binary: "sleep".into(),
                #[cfg(not(windows))]
                args: vec!["30".into()],
                cwd: None,
                stdin: None,
                // Below the 1s floor → clamped to 1s; process runs 30s.
                timeout_ms: Some(1),
                env: None,
                operation: None,
            },
            None,
        )
        .await;
        assert!(result.timed_out, "{result:?}");
        assert_eq!(result.exit_code, None);
    }

    #[test]
    fn lane_key_scopes_by_cwd_and_operation() {
        assert_ne!(
            lane_key(Some("C:/a"), Some("commit")),
            lane_key(Some("C:/b"), Some("commit"))
        );
        assert_eq!(
            lane_key(Some("C:/a"), Some("commit")),
            lane_key(Some("C:/a"), Some("commit"))
        );
    }

    #[test]
    fn cancel_reports_whether_a_job_was_live() {
        let registry = LaneRegistry::new();
        assert!(!registry.cancel(&lane_key(Some("x"), None)));
        let _rx = registry.register(&lane_key(Some("x"), None));
        assert!(registry.cancel(&lane_key(Some("x"), None)));
    }
}
