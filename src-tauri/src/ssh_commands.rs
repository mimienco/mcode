//! Tauri commands over the `mcode-ssh` crate (PTY_CONTRACT_SPEC.md §9.3:
//! SSH pty ownership is "owned by the SSH spec" — the command layer here
//! mirrors `pty_commands.rs` so the renderer sees one idiom for every
//! terminal, local or remote).
//!
//! Every command addresses a connection or pty by an opaque handle; the
//! renderer never sees a russh handle. Output forwarding mirrors the local
//! byte owner's batch shape on `pty://replay` (`{ptyId, data}`), and death is
//! announced exactly once via `pty://exit` after a final flush — the renderer
//! tears the terminal down on exit, so any batched output not yet flushed
//! would be lost. Host-key first contact follows the original TOFU flow
//! (`ssh-connection.ts` + `ssh-host-key-store.ts`): connect reports
//! `host-key-unknown` with a `connectAttemptId`, the user decides, and the
//! connect re-runs against a store that now holds the accepted key.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcode_ssh::SshError;
use mcode_ssh::pty_id::{is_remote_pty_id, to_app_ssh_pty_id};
use mcode_ssh::session::SshPtyChannel;
use mcode_ssh::store::HostKeyStore;
use mcode_ssh::transport::{
    ConnectOutcome, HostKeyTrust, SshConnection, SshTarget, accept_host_key,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::DesktopCommandError;

#[path = "ssh_pty_exit_monitor.rs"]
mod exit_monitor;

/// Renderer-facing event names (`pty://replay` carries SSH pty output; the
/// exit announcement reuses the local pty's `pty://exit` shape with a string
/// `ptyId`).
pub(crate) const SSH_PTY_REPLAY_EVENT: &str = "pty://replay";
pub(crate) const SSH_PTY_EXIT_EVENT: &str = "pty://exit";

const SSH_CONNECT_OPERATION: &str = "ssh_connect";
const SSH_ACCEPT_HOST_KEY_OPERATION: &str = "ssh_accept_host_key";
const SSH_EXEC_OPERATION: &str = "ssh_exec";
const SSH_OPEN_PTY_OPERATION: &str = "ssh_open_pty";
const SSH_WRITE_PTY_OPERATION: &str = "ssh_write_pty";
const SSH_RESIZE_PTY_OPERATION: &str = "ssh_resize_pty";
const SSH_CLOSE_PTY_OPERATION: &str = "ssh_close_pty";
const SSH_DISCONNECT_OPERATION: &str = "ssh_disconnect";

/// `ssh_exec` deadline: default and clamp bounds, matching the renderer's
/// `ssh-client.ts` (`120s` default, `1s..600s` clamp).
const DEFAULT_EXEC_TIMEOUT_MS: u64 = 120_000;
const MIN_EXEC_TIMEOUT_MS: u64 = 1_000;
const MAX_EXEC_TIMEOUT_MS: u64 = 600_000;

/// Forwarder read buffer: one `SshPtyChannel::read` call per lock window.
const SSH_PTY_READ_BUFFER_BYTES: usize = 16 * 1024;
/// Forwarder cadence: a batch never lingers longer than one interval — the
/// read lock window doubles as the flush timer (the local `pty://data`
/// lane's 2 ms gating policy).
const SSH_PTY_FLUSH_INTERVAL: Duration = Duration::from_millis(2);

// ─── Wire DTOs ──────────────────────────────────────────────────────────────

/// Renderer-facing connect request, mirroring `SshTarget`. Auth is an
/// internally-tagged enum: `{type:'key',keyPath,passphrase?}`,
/// `{type:'password',password}` or `{type:'agent'}`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTargetDto {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuthDto,
    pub connect_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SshAuthDto {
    Key {
        key_path: String,
        passphrase: Option<String>,
    },
    Password {
        password: String,
    },
    Agent,
}

impl SshTargetDto {
    /// Lowers the wire DTO onto the crate's `SshTarget` unchanged — the
    /// transport validates host/port/username/key path itself and reports
    /// `ssh_invalid_argument` through the shared error mapping.
    fn to_ssh_target(&self) -> SshTarget {
        SshTarget {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth: match &self.auth {
                SshAuthDto::Key {
                    key_path,
                    passphrase,
                } => mcode_ssh::SshAuth::Key {
                    path: key_path.clone().into(),
                    passphrase: passphrase.clone(),
                },
                SshAuthDto::Password { password } => mcode_ssh::SshAuth::Password {
                    password: password.clone(),
                },
                SshAuthDto::Agent => mcode_ssh::SshAuth::Agent,
            },
            connect_timeout: self.connect_timeout_ms.map(Duration::from_millis),
        }
    }
}

/// Connect verdict union. `status` is the kebab-case discriminator
/// (`connected`, `host-key-unknown`, `host-key-changed`, `rejected`); for
/// the host-key outcomes the presented key rides along so the renderer can
/// render the TOFU prompt without a second round trip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum SshConnectResponse {
    Connected {
        connection_id: u64,
    },
    HostKeyUnknown {
        connect_attempt_id: u64,
        fingerprint: String,
        algorithm: String,
        key_base64: String,
    },
    HostKeyChanged {
        fingerprint: String,
        algorithm: String,
    },
    Rejected,
}

/// `ssh_exec` response: lossy-UTF-8 streams and a clamped exit status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshExecResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// `ssh_open_pty` response: the app-scoped pty id
/// (`ssh:<connectionId>@@pty-<n>`), the routing key every subsequent
/// write/resize/close/event carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshOpenPtyResponse {
    pub pty_id: String,
}

/// `pty://replay` payload — the string app pty id plus the lossy batch.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyReplayEvent {
    pty_id: String,
    data: String,
}

/// `pty://exit` payload (`{ptyId, code}`), announced exactly once per pty.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyExitEvent {
    pty_id: String,
    code: i32,
}

// ─── Managed state ──────────────────────────────────────────────────────────

/// One pending first-contact decision: the exact target the user was
/// connecting to (retried verbatim after an accept) and the presented key
/// (persisted into the store on accept).
struct PendingHostKeyAttempt {
    target: SshTargetDto,
    presented: mcode_ssh::transport::PresentedHostKey,
}

struct SshStateInner {
    connections: Mutex<HashMap<u64, Arc<tokio::sync::Mutex<SshConnection>>>>,
    ptys: Mutex<HashMap<String, Arc<tokio::sync::Mutex<SshPtyChannel>>>>,
    pending_host_keys: Mutex<HashMap<u64, PendingHostKeyAttempt>>,
    next_id: AtomicU64,
}

impl std::fmt::Debug for SshStateInner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let connections = self
            .connections
            .lock()
            .map(|connections| connections.len())
            .unwrap_or_default();
        let ptys = self.ptys.lock().map(|ptys| ptys.len()).unwrap_or_default();
        let pending = self
            .pending_host_keys
            .lock()
            .map(|pending| pending.len())
            .unwrap_or_default();
        formatter
            .debug_struct("SshStateInner")
            .field("live_connections", &connections)
            .field("live_ptys", &ptys)
            .field("pending_host_keys", &pending)
            .field("next_id", &self.next_id.load(Ordering::SeqCst))
            .finish()
    }
}

/// Tauri-managed SSH state shared by every `ssh_*` command.
///
/// Connections are `Arc<Mutex<SshConnection>>` because `exec` and
/// `open_pty_channel` take `&self` (russh's handle is multi-channel) while
/// `disconnect` needs `&mut` — the per-connection mutex is the only way to
/// hand that out safely, and it is never held across the map lock.
#[derive(Clone, Debug)]
pub struct SshState(Arc<SshStateInner>);

impl SshState {
    pub fn new() -> Self {
        Self(Arc::new(SshStateInner {
            connections: Mutex::new(HashMap::new()),
            ptys: Mutex::new(HashMap::new()),
            pending_host_keys: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }))
    }

    fn allocate_id(&self) -> u64 {
        self.0.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn insert_connection(&self, id: u64, connection: SshConnection) {
        self.0
            .connections
            .lock()
            .expect("ssh connections lock")
            .insert(id, Arc::new(tokio::sync::Mutex::new(connection)));
    }

    fn get_connection(&self, id: u64) -> Option<Arc<tokio::sync::Mutex<SshConnection>>> {
        self.0
            .connections
            .lock()
            .expect("ssh connections lock")
            .get(&id)
            .cloned()
    }

    fn remove_connection(&self, id: u64) -> Option<Arc<tokio::sync::Mutex<SshConnection>>> {
        self.0
            .connections
            .lock()
            .expect("ssh connections lock")
            .remove(&id)
    }

    fn get_pty(&self, pty_id: &str) -> Option<Arc<tokio::sync::Mutex<SshPtyChannel>>> {
        self.0
            .ptys
            .lock()
            .expect("ssh ptys lock")
            .get(pty_id)
            .cloned()
    }

    fn insert_pty(&self, pty_id: String, channel: Arc<tokio::sync::Mutex<SshPtyChannel>>) {
        self.0
            .ptys
            .lock()
            .expect("ssh ptys lock")
            .insert(pty_id, channel);
    }

    fn remove_pty(&self, pty_id: &str) -> Option<Arc<tokio::sync::Mutex<SshPtyChannel>>> {
        self.0.ptys.lock().expect("ssh ptys lock").remove(pty_id)
    }

    fn has_pty_instance(
        &self,
        pty_id: &str,
        channel: &Arc<tokio::sync::Mutex<SshPtyChannel>>,
    ) -> bool {
        self.0
            .ptys
            .lock()
            .expect("ssh ptys lock")
            .get(pty_id)
            .is_some_and(|current| Arc::ptr_eq(current, channel))
    }

    fn remove_pty_instance(
        &self,
        pty_id: &str,
        channel: &Arc<tokio::sync::Mutex<SshPtyChannel>>,
    ) -> bool {
        let mut ptys = self.0.ptys.lock().expect("ssh ptys lock");
        if ptys
            .get(pty_id)
            .is_some_and(|current| Arc::ptr_eq(current, channel))
        {
            ptys.remove(pty_id);
            true
        } else {
            false
        }
    }

    fn insert_pending_host_key(
        &self,
        target: SshTargetDto,
        presented: mcode_ssh::transport::PresentedHostKey,
    ) -> u64 {
        let id = self.allocate_id();
        self.0
            .pending_host_keys
            .lock()
            .expect("ssh pending host keys lock")
            .insert(id, PendingHostKeyAttempt { target, presented });
        id
    }

    fn take_pending_host_key(&self, id: u64) -> Option<PendingHostKeyAttempt> {
        self.0
            .pending_host_keys
            .lock()
            .expect("ssh pending host keys lock")
            .remove(&id)
    }
}

impl Default for SshState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Error mapping ──────────────────────────────────────────────────────────

fn ssh_error(error: SshError, operation: &'static str) -> DesktopCommandError {
    DesktopCommandError::new(
        error.code(),
        operation,
        error.to_string(),
        user_message_for(operation),
        None,
        None,
    )
}

fn command_error(
    code: &'static str,
    operation: &'static str,
    message: impl Into<String>,
) -> DesktopCommandError {
    DesktopCommandError::new(
        code,
        operation,
        message.into(),
        user_message_for(operation),
        None,
        None,
    )
}

fn user_message_for(operation: &'static str) -> &'static str {
    match operation {
        SSH_CONNECT_OPERATION => "MCode could not connect to this SSH host. Try again.",
        SSH_ACCEPT_HOST_KEY_OPERATION => "MCode could not save this host key decision. Try again.",
        SSH_EXEC_OPERATION => "MCode could not run this command on the SSH host. Try again.",
        SSH_OPEN_PTY_OPERATION => "MCode could not open this SSH terminal. Try again.",
        SSH_WRITE_PTY_OPERATION => {
            "MCode could not send this input to the SSH terminal. Try again."
        }
        SSH_RESIZE_PTY_OPERATION => "MCode could not resize this SSH terminal. Try again.",
        SSH_CLOSE_PTY_OPERATION => "MCode could not close this SSH terminal. Try again.",
        SSH_DISCONNECT_OPERATION => "MCode could not close this SSH connection. Try again.",
        _ => "MCode could not complete the SSH request. Try again.",
    }
}

/// Trust sources for a connect: MCode's accepted-key store beside the app
/// data file (`ssh-host-keys.json` next to `mcode-data.json`, the original
/// `ssh-host-key-store.ts:48-50` convention) plus the user's read-only
/// `known_hosts` ladder. No usable data dir (no home) is a hard failure —
/// answering "nothing trusted" would turn every host into first contact.
fn host_key_trust(
    app: &AppHandle,
    operation: &'static str,
) -> Result<HostKeyTrust, DesktopCommandError> {
    let data_dir = app.path().app_data_dir().map_err(|error| {
        command_error(
            "ssh_store_unavailable",
            operation,
            format!("app data directory unavailable: {error}"),
        )
    })?;
    let store_path = data_dir.join("ssh-host-keys.json");
    Ok(HostKeyTrust {
        store: Arc::new(HostKeyStore::open(store_path)),
        known_hosts_paths: mcode_ssh::default_known_hosts_files(),
    })
}

// ─── Commands ───────────────────────────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_connect(
    app: AppHandle,
    state: tauri::State<'_, SshState>,
    target: SshTargetDto,
) -> Result<SshConnectResponse, DesktopCommandError> {
    let trust = host_key_trust(&app, SSH_CONNECT_OPERATION)?;
    ssh_connect_for(&state, target, trust).await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_accept_host_key(
    app: AppHandle,
    state: tauri::State<'_, SshState>,
    connect_attempt_id: u64,
    accept: bool,
) -> Result<SshConnectResponse, DesktopCommandError> {
    let trust = host_key_trust(&app, SSH_ACCEPT_HOST_KEY_OPERATION)?;
    ssh_accept_host_key_for(&state, connect_attempt_id, accept, trust).await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_exec(
    state: tauri::State<'_, SshState>,
    connection_id: u64,
    command: String,
    timeout_ms: Option<u64>,
) -> Result<SshExecResponse, DesktopCommandError> {
    ssh_exec_for(&state, connection_id, command, timeout_ms).await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_open_pty(
    app: AppHandle,
    state: tauri::State<'_, SshState>,
    connection_id: u64,
    cols: u16,
    rows: u16,
) -> Result<SshOpenPtyResponse, DesktopCommandError> {
    let response = ssh_open_pty_for(&state, connection_id, cols, rows).await?;
    if let Some(channel) = state.get_pty(&response.pty_id) {
        spawn_ssh_pty_forwarder(app, response.pty_id.clone(), channel, state.inner().clone());
    }
    Ok(response)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_write_pty(
    state: tauri::State<'_, SshState>,
    pty_id: String,
    data: String,
) -> Result<(), DesktopCommandError> {
    ssh_write_pty_for(&state, pty_id, data).await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_resize_pty(
    state: tauri::State<'_, SshState>,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), DesktopCommandError> {
    ssh_resize_pty_for(&state, pty_id, cols, rows).await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_close_pty(
    state: tauri::State<'_, SshState>,
    pty_id: String,
) -> Result<(), DesktopCommandError> {
    ssh_close_pty_for(&state, pty_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn ssh_disconnect(
    state: tauri::State<'_, SshState>,
    connection_id: u64,
) -> Result<(), DesktopCommandError> {
    ssh_disconnect_for(&state, connection_id).await
}

// ─── Command seams (renderer-contract tests) ────────────────────────────────

/// `ssh_connect` body without the Tauri runtime. On first contact the
/// presented key is retained under a fresh `connectAttemptId` and the
/// half-open connection is reported (the transport already tore it down); a
/// changed key is always refused. Trust comes from the caller so tests can
/// point the store at a temp dir.
pub async fn ssh_connect_for(
    state: &SshState,
    target: SshTargetDto,
    trust: HostKeyTrust,
) -> Result<SshConnectResponse, DesktopCommandError> {
    let ssh_target = target.to_ssh_target();
    match SshConnection::connect(&ssh_target, &trust).await {
        Ok(ConnectOutcome::Connected(connection)) => {
            let connection_id = state.allocate_id();
            state.insert_connection(connection_id, connection);
            Ok(SshConnectResponse::Connected { connection_id })
        }
        Ok(ConnectOutcome::HostKeyUnknown(presented)) => {
            let connect_attempt_id = state.insert_pending_host_key(target, presented.clone());
            Ok(SshConnectResponse::HostKeyUnknown {
                connect_attempt_id,
                fingerprint: presented.fingerprint,
                algorithm: presented.algorithm.as_str().to_owned(),
                key_base64: presented.key_base64,
            })
        }
        Ok(ConnectOutcome::HostKeyChanged { presented, .. }) => {
            Ok(SshConnectResponse::HostKeyChanged {
                fingerprint: presented.fingerprint,
                algorithm: presented.algorithm.as_str().to_owned(),
            })
        }
        Err(error) => Err(ssh_error(error, SSH_CONNECT_OPERATION)),
    }
}

/// `ssh_accept_host_key` body without the Tauri runtime: `accept=false`
/// drops the pending attempt and reports `rejected`; `accept=true` persists
/// the presented key into the store and re-runs the connect with the same
/// target (the original's TOFU flow — the retry passes because the store now
/// holds the key). If the retry still reports a host-key verdict (TOFU race
/// or a genuinely changed key), that verdict surfaces as-is; this never
/// loops internally.
pub async fn ssh_accept_host_key_for(
    state: &SshState,
    connect_attempt_id: u64,
    accept: bool,
    trust: HostKeyTrust,
) -> Result<SshConnectResponse, DesktopCommandError> {
    let Some(attempt) = state.take_pending_host_key(connect_attempt_id) else {
        // Unknown (already consumed or never issued): nothing to accept.
        return Ok(SshConnectResponse::Rejected);
    };
    if !accept {
        return Ok(SshConnectResponse::Rejected);
    }
    accept_host_key(
        &trust,
        &attempt.target.host,
        attempt.target.port,
        &attempt.presented,
    )
    .await
    .map_err(|error| ssh_error(error, SSH_ACCEPT_HOST_KEY_OPERATION))?;
    ssh_connect_for(state, attempt.target, trust).await
}

/// `ssh_exec` body without the Tauri runtime: runs one command to
/// completion on the connection. Unknown ids report `ssh_connection_gone`.
pub async fn ssh_exec_for(
    state: &SshState,
    connection_id: u64,
    command: String,
    timeout_ms: Option<u64>,
) -> Result<SshExecResponse, DesktopCommandError> {
    let connection = state.get_connection(connection_id).ok_or_else(|| {
        command_error(
            "ssh_connection_gone",
            SSH_EXEC_OPERATION,
            "ssh connection is gone",
        )
    })?;
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_EXEC_TIMEOUT_MS)
            .clamp(MIN_EXEC_TIMEOUT_MS, MAX_EXEC_TIMEOUT_MS),
    );
    let deadline = tokio::time::Instant::now() + timeout;
    let connection = lock_ssh_exec_connection(&connection, deadline).await?;
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Err(ssh_error(
            mcode_ssh::SshError::Timeout("exec"),
            SSH_EXEC_OPERATION,
        ));
    }
    let output = connection
        .exec(&command, remaining)
        .await
        .map_err(|error| ssh_error(error, SSH_EXEC_OPERATION))?;
    Ok(SshExecResponse {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: i32::try_from(output.exit_code).unwrap_or(i32::MAX),
    })
}

async fn lock_ssh_exec_connection<T>(
    connection: &tokio::sync::Mutex<T>,
    deadline: tokio::time::Instant,
) -> Result<tokio::sync::MutexGuard<'_, T>, DesktopCommandError> {
    tokio::time::timeout_at(deadline, connection.lock())
        .await
        .map_err(|_| ssh_error(mcode_ssh::SshError::Timeout("exec"), SSH_EXEC_OPERATION))
}

#[cfg(test)]
mod queued_exec_tests {
    use super::*;

    #[tokio::test]
    async fn queued_exec_expires_without_acquiring_busy_connection() {
        let connection = tokio::sync::Mutex::new(());
        let held = connection.lock().await;
        let result = lock_ssh_exec_connection(
            &connection,
            tokio::time::Instant::now() + Duration::from_millis(10),
        )
        .await;
        assert_eq!(result.unwrap_err().code, "ssh_timeout");
        drop(held);
        assert!(connection.try_lock().is_ok());
    }
}

/// `ssh_open_pty` body without the Tauri runtime: opens the channel,
/// allocates the app-scoped pty id through `to_app_ssh_pty_id` (the plain
/// decimal connection id round-trips the execution-host normalization) and
/// registers it. The caller spawns the forwarder — it needs the `AppHandle`
/// for emits, which tests running without the Tauri runtime do not have.
pub async fn ssh_open_pty_for(
    state: &SshState,
    connection_id: u64,
    cols: u16,
    rows: u16,
) -> Result<SshOpenPtyResponse, DesktopCommandError> {
    let connection = state.get_connection(connection_id).ok_or_else(|| {
        command_error(
            "ssh_connection_gone",
            SSH_OPEN_PTY_OPERATION,
            "ssh connection is gone",
        )
    })?;
    let channel = connection
        .lock()
        .await
        .open_pty_channel(u32::from(cols), u32::from(rows))
        .await
        .map_err(|error| ssh_error(error, SSH_OPEN_PTY_OPERATION))?;
    let pty_number = state.allocate_id();
    let pty_id = to_app_ssh_pty_id(&connection_id.to_string(), &format!("pty-{pty_number}"))
        .map_err(|error| ssh_error(error, SSH_OPEN_PTY_OPERATION))?;
    state.insert_pty(pty_id.clone(), Arc::new(tokio::sync::Mutex::new(channel)));
    Ok(SshOpenPtyResponse { pty_id })
}

/// `ssh_write_pty` body without the Tauri runtime. `remote:`-prefixed ids
/// are runtime terminal handles owned by the local provider's
/// `terminal.close` — they never name an SSH pty, so they are rejected with
/// the same `ssh_pty_gone` code and a context message saying why.
pub async fn ssh_write_pty_for(
    state: &SshState,
    pty_id: String,
    data: String,
) -> Result<(), DesktopCommandError> {
    if is_remote_pty_id(&pty_id) {
        return Err(command_error(
            "ssh_pty_gone",
            SSH_WRITE_PTY_OPERATION,
            format!("'{pty_id}' is a runtime terminal handle, not an SSH pty"),
        ));
    }
    let channel = state
        .get_pty(&pty_id)
        .ok_or_else(|| command_error("ssh_pty_gone", SSH_WRITE_PTY_OPERATION, "ssh pty is gone"))?;
    channel
        .lock()
        .await
        .write(data.as_bytes())
        .await
        .map_err(|error| ssh_error(error, SSH_WRITE_PTY_OPERATION))
}

/// `ssh_resize_pty` body without the Tauri runtime. Unknown ids report
/// `ssh_pty_gone`.
pub async fn ssh_resize_pty_for(
    state: &SshState,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), DesktopCommandError> {
    let channel = state.get_pty(&pty_id).ok_or_else(|| {
        command_error("ssh_pty_gone", SSH_RESIZE_PTY_OPERATION, "ssh pty is gone")
    })?;
    channel
        .lock()
        .await
        .resize(u32::from(cols), u32::from(rows))
        .await
        .map_err(|error| ssh_error(error, SSH_RESIZE_PTY_OPERATION))
}

/// `ssh_close_pty` body without the Tauri runtime. Idempotent: an unknown
/// id already closed, like `pty_kill`.
pub async fn ssh_close_pty_for(
    state: &SshState,
    pty_id: String,
) -> Result<(), DesktopCommandError> {
    let Some(channel) = state.remove_pty(&pty_id) else {
        return Ok(());
    };
    channel
        .lock()
        .await
        .close()
        .await
        .map_err(|error| ssh_error(error, SSH_CLOSE_PTY_OPERATION))
}

/// `ssh_disconnect` body without the Tauri runtime. Idempotent: an unknown
/// id already disconnected. The connection handle is taken out of the map
/// before awaiting so the map lock is never held across the disconnect.
pub async fn ssh_disconnect_for(
    state: &SshState,
    connection_id: u64,
) -> Result<(), DesktopCommandError> {
    let Some(connection) = state.remove_connection(connection_id) else {
        return Ok(());
    };
    connection
        .lock()
        .await
        .disconnect()
        .await
        .map_err(|error| ssh_error(error, SSH_DISCONNECT_OPERATION))
}

// ─── Output forwarding ──────────────────────────────────────────────────────

/// One forwarder per SSH pty: reads through the channel's 16 KiB buffer,
/// accumulates raw bytes (byte boundaries preserved — a partial multi-byte
/// sequence stays pending until it completes) and emits `pty://replay` at
/// most every 2 ms. On EOF it flushes the remainder lossily, awaits the
/// host exit evidence before announcing `pty://exit`. Transport loss leaves
/// the identity present and reports write unavailability as `unverifiable`.
fn spawn_ssh_pty_forwarder(
    app: AppHandle,
    pty_id: String,
    channel: Arc<tokio::sync::Mutex<SshPtyChannel>>,
    state: SshState,
) {
    tauri::async_runtime::spawn(async move {
        let mut pending: Vec<u8> = Vec::new();
        let mut buffer = vec![0u8; SSH_PTY_READ_BUFFER_BYTES];

        loop {
            if !state.has_pty_instance(&pty_id, &channel) {
                return;
            }
            // The lock window bounds the read at the flush cadence: a quiet
            // stream times out the 2 ms window and falls through to the emit
            // below, so a batch never lingers longer than one interval.
            let read = {
                let Ok(mut guard) =
                    tokio::time::timeout(Duration::from_millis(100), channel.lock()).await
                else {
                    continue;
                };
                tokio::time::timeout(SSH_PTY_FLUSH_INTERVAL, guard.read(&mut buffer)).await
            };
            match read {
                Ok(Ok(0)) => break, // EOF: the remote side closed the stream
                Ok(Ok(read_bytes)) => {
                    pending.extend_from_slice(&buffer[..read_bytes]);
                }
                Ok(Err(error)) => {
                    eprintln!("[ssh-pty] read failed for {pty_id}: {error}");
                    break;
                }
                Err(_elapsed) => {} // flush window: fall through to the emit below
            }
            if !pending.is_empty() {
                let data = flush_lossy_prefix(&mut pending);
                let event = PtyReplayEvent {
                    pty_id: pty_id.clone(),
                    data,
                };
                if let Err(error) = app.emit(SSH_PTY_REPLAY_EVENT, &event) {
                    eprintln!("[ssh-pty] failed to emit replay event for {pty_id}: {error}");
                }
            }
        }

        // Final flush: the stream ended, so a trailing partial sequence can
        // never complete — flush everything lossily.
        if !pending.is_empty() {
            let data = String::from_utf8_lossy(&pending).into_owned();
            let event = PtyReplayEvent {
                pty_id: pty_id.clone(),
                data,
            };
            if let Err(error) = app.emit(SSH_PTY_REPLAY_EVENT, &event) {
                eprintln!("[ssh-pty] failed to emit final replay event for {pty_id}: {error}");
            }
        }

        let grace_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut monitor = exit_monitor::ExitMonitor::default();
        loop {
            if !state.has_pty_instance(&pty_id, &channel) {
                return;
            }
            let (evidence, closed) = {
                let Ok(mut guard) =
                    tokio::time::timeout(Duration::from_millis(100), channel.lock()).await
                else {
                    continue;
                };
                if !state.has_pty_instance(&pty_id, &channel) {
                    return;
                }
                // Cancellation retains received evidence and releases the lock for explicit close/write.
                let _ = tokio::time::timeout(Duration::from_millis(100), guard.wait()).await;
                (guard.exit_evidence(), guard.is_closed())
            };
            match monitor.observe(
                state.has_pty_instance(&pty_id, &channel),
                &evidence,
                closed,
                tokio::time::Instant::now() >= grace_deadline,
            ) {
                exit_monitor::Decision::Wait => {}
                exit_monitor::Decision::Stop => return,
                exit_monitor::Decision::Unverifiable { finished } => {
                    if !state.has_pty_instance(&pty_id, &channel) {
                        return;
                    }
                    let event = serde_json::json!({ "ptyId": pty_id, "reason": "unverifiable" });
                    if let Err(error) =
                        app.emit(crate::pty_commands::PTY_WRITE_UNAVAILABLE_EVENT, &event)
                    {
                        eprintln!("[ssh-pty] failed to emit transport loss: {error}");
                    }
                    if finished {
                        return;
                    }
                }
                exit_monitor::Decision::Exited(code) => {
                    if state.remove_pty_instance(&pty_id, &channel) {
                        let event = PtyExitEvent {
                            pty_id: pty_id.clone(),
                            code,
                        };
                        if let Err(error) = app.emit(SSH_PTY_EXIT_EVENT, &event) {
                            eprintln!("[ssh-pty] failed to emit exit event for {pty_id}: {error}");
                        }
                    }
                    return;
                }
            }
        }
    });
}

fn confirmed_ssh_exit_code(evidence: &mcode_ssh::session::PtyWaitResult) -> Option<i32> {
    evidence
        .exit_code
        .map(|code| code.min(i32::MAX as u32) as i32)
        .or_else(|| evidence.exit_signal.as_ref().map(|_| -1))
}

/// Converts the longest valid UTF-8 prefix of `pending` to a String and
/// drains it, leaving any partial multi-byte tail buffered for the next
/// flush so byte boundaries are never torn.
fn flush_lossy_prefix(pending: &mut Vec<u8>) -> String {
    let mut offset = 0;
    let mut valid_up_to = pending.len();
    while let Err(error) = std::str::from_utf8(&pending[offset..]) {
        offset += error.valid_up_to();
        match error.error_len() {
            Some(length) => offset += length,
            None => {
                valid_up_to = offset;
                break;
            }
        }
    }
    let data = String::from_utf8_lossy(&pending[..valid_up_to]).into_owned();
    pending.drain(..valid_up_to);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_loss_cannot_be_reported_as_process_exit() {
        use mcode_ssh::session::PtyWaitResult;
        assert_eq!(
            confirmed_ssh_exit_code(&PtyWaitResult {
                exit_code: None,
                exit_signal: None
            }),
            None
        );
        assert_eq!(
            confirmed_ssh_exit_code(&PtyWaitResult {
                exit_code: Some(7),
                exit_signal: None
            }),
            Some(7)
        );
        assert_eq!(
            confirmed_ssh_exit_code(&PtyWaitResult {
                exit_code: None,
                exit_signal: Some("TERM".into())
            }),
            Some(-1)
        );
    }

    #[test]
    fn lossy_prefix_preserves_partial_byte_boundaries() {
        let mut pending = b"caf\xc3".to_vec();
        let data = flush_lossy_prefix(&mut pending);
        assert_eq!(data, "caf");
        assert_eq!(pending, b"\xc3");
    }

    #[test]
    fn invalid_utf8_does_not_block_later_output() {
        let mut pending = b"a\xffb\xc3".to_vec();
        assert_eq!(flush_lossy_prefix(&mut pending), "a\u{fffd}b");
        assert_eq!(pending, b"\xc3");
        pending.push(0xa9);
        assert_eq!(flush_lossy_prefix(&mut pending), "é");
        assert!(pending.is_empty());
    }
}
