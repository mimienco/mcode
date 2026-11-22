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

use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcode_process::exec_tree::ExecTree;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub operation: Option<String>,
}

/// agent-exec-handler.ts:85 — one visible non-interactive job per cwd+operation.
pub fn lane_key(cwd: Option<&str>, operation: Option<&str>) -> String {
    serde_json::to_string(&[
        operation
            .filter(|value| !value.is_empty())
            .unwrap_or("default"),
        cwd.unwrap_or_default(),
    ])
    .expect("string tuple serializes")
}

fn clamp_timeout(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1_000, MAX_TIMEOUT_MS)
}

pub struct LaneRegistry {
    in_flight: Mutex<HashMap<String, LaneEntry>>,
}

struct LaneEntry {
    identity: Arc<()>,
    sender: tokio::sync::watch::Sender<bool>,
}

pub struct LaneLease<'a> {
    registry: &'a LaneRegistry,
    key: String,
    identity: Arc<()>,
    receiver: tokio::sync::watch::Receiver<bool>,
}

impl Drop for LaneLease<'_> {
    fn drop(&mut self) {
        let mut entries = self
            .registry
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries
            .get(&self.key)
            .is_some_and(|entry| Arc::ptr_eq(&entry.identity, &self.identity))
        {
            entries.remove(&self.key);
        }
    }
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
    pub fn register(&self, key: &str) -> LaneLease<'_> {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let identity = Arc::new(());
        if let Some(prior) = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                key.to_string(),
                LaneEntry {
                    identity: identity.clone(),
                    sender: tx,
                },
            )
        {
            prior.sender.send_replace(true);
        }
        LaneLease {
            registry: self,
            key: key.to_owned(),
            identity,
            receiver: rx,
        }
    }

    /// agent-exec-handler.ts:47-58 — cancel resolves `{ canceled: true }` only
    /// when a job is live.
    pub fn cancel(&self, key: &str) -> bool {
        match self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key)
        {
            Some(entry) => {
                entry.sender.send_replace(true);
                true
            }
            None => false,
        }
    }
}

async fn wait_for_cancel(receiver: Option<tokio::sync::watch::Receiver<bool>>) {
    if let Some(mut receiver) = receiver {
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            if receiver.changed().await.is_err() {
                break;
            }
        }
    }
    std::future::pending::<()>().await;
}

fn spawn_failure(error: impl ToString) -> ExecResult {
    ExecResult {
        spawn_error: Some(error.to_string()),
        ..ExecResult::default()
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
    let lane = if params.cwd.as_deref().is_some_and(|cwd| !cwd.is_empty()) {
        Some(lane_key(params.cwd.as_deref(), params.operation.as_deref()))
    } else {
        None
    };
    let mut command = Command::new(&params.binary);
    command
        .args(&params.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = params.cwd.as_ref().filter(|cwd| !cwd.is_empty()) {
        command.current_dir(cwd);
    }
    for (key, value) in params.env.iter().flatten() {
        command.env(key, value);
    }

    let prepared_tree = match ExecTree::prepare(command.as_std_mut()) {
        Ok(tree) => tree,
        Err(error) => return spawn_failure(error),
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return spawn_failure(error);
        }
    };

    // Drop tree ownership before dropping the child, whose reaping could release its pid.
    let mut tree = prepared_tree;
    if let Err(error) = tree.attach(child.id().expect("newly spawned child has pid")) {
        tree.terminate();
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        return spawn_failure(error);
    }
    let lease = lane.as_ref().map(|key| registry.register(key));
    let canceled = async {
        tokio::select! {
            _ = wait_for_cancel(lease.as_ref().map(|lease| lease.receiver.clone())) => {},
            _ = wait_for_cancel(extra_canceled_rx) => {},
        }
    };
    let stdin = child.stdin.take();
    let write_stdin = async {
        if let Some(mut stdin) = stdin {
            let _ = stdin
                .write_all(params.stdin.as_deref().unwrap_or_default().as_bytes())
                .await;
            let _ = stdin.shutdown().await;
        }
    };
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut stdout_chunk = [0_u8; 16 * 1024];
    let mut stderr_chunk = [0_u8; 16 * 1024];
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut stdin_closed = false;
    let mut result = ExecResult::default();
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline, canceled, write_stdin);
    loop {
        tokio::select! {
            biased;
            _ = &mut canceled => { result.canceled = Some(true); break; },
            _ = &mut deadline => { result.timed_out = true; break; },
            read = stdout.read(&mut stdout_chunk), if !stdout_closed => {
                match read {
                    Ok(0) => stdout_closed = true,
                    Ok(count) if stdout_bytes.len() + count <= MAX_OUTPUT_BYTES => stdout_bytes.extend_from_slice(&stdout_chunk[..count]),
                    Ok(_) => break,
                    Err(error) => { result.spawn_error = Some(error.to_string()); break; },
                }
            },
            read = stderr.read(&mut stderr_chunk), if !stderr_closed => {
                match read {
                    Ok(0) => stderr_closed = true,
                    Ok(count) if stderr_bytes.len() + count <= MAX_OUTPUT_BYTES => stderr_bytes.extend_from_slice(&stderr_chunk[..count]),
                    Ok(_) => break,
                    Err(error) => { result.spawn_error = Some(error.to_string()); break; },
                }
            },
            _ = &mut write_stdin, if !stdin_closed => stdin_closed = true,
            // Delay reaping until inherited pipes close so tree termination retains pid ownership.
            status = child.wait(), if stdout_closed && stderr_closed => {
                match status {
                    Ok(status) => { result.exit_code = status.code(); tree.disarm(); },
                    Err(error) => result.spawn_error = Some(error.to_string()),
                }
                break;
            },
        }
    }
    tree.terminate();
    if result.exit_code.is_none() {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    result.stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    result.stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    result
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

    #[test]
    fn old_lane_completion_cannot_unregister_its_replacement() {
        let registry = LaneRegistry::new();
        let key = lane_key(Some("x"), None);
        let old = registry.register(&key);
        let newest = registry.register(&key);
        assert!(*old.receiver.borrow());
        assert!(!*newest.receiver.borrow());
        drop(old);
        assert!(registry.cancel(&key));
        assert!(*newest.receiver.borrow());
        drop(newest);
        assert!(!registry.cancel(&key));
    }

    #[test]
    fn default_operation_aliases_match_electron() {
        assert_eq!(
            lane_key(Some("x"), None),
            lane_key(Some("x"), Some("default"))
        );
        assert_eq!(lane_key(Some("x"), None), lane_key(Some("x"), Some("")));
    }

    #[test]
    fn existing_environment_object_wire_shape_round_trips() {
        let value = serde_json::json!({"binary":"agent", "env":{"CODEX_HOME":"/managed/account", "PATH":"/managed/bin"}});
        let params: ExecParams = serde_json::from_value(value).unwrap();
        assert_eq!(
            params.env.as_ref().unwrap()["CODEX_HOME"],
            "/managed/account"
        );
        assert_eq!(
            serde_json::to_value(params).unwrap()["env"],
            serde_json::json!({"CODEX_HOME":"/managed/account", "PATH":"/managed/bin"})
        );
    }
}
