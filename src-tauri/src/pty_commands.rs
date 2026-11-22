//! Tauri commands over the `mcode-pty` crate (spec
//! `docs/migration/PTY_CONTRACT_SPEC.md` §4 CORE rows: `pty:spawn`,
//! `pty:data`, `pty:write`, `pty:resize`, `pty:kill`, `pty:exit`).
//!
//! Every command addresses a session by its opaque handle; the renderer never
//! sees a pid or native handle (spec §2). Output forwarding mirrors the byte
//! owner's `take_batch` shape on `pty://data`, and death is announced exactly
//! once via `pty://exit` after a final flush — the renderer tears the terminal
//! down on exit, so any batched output not yet flushed would be lost
//! (spec §4 `pty:exit` row, `pty.ts:3730-3757`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcode_pty::{OutputBatch, PtyError, PtyHandle, PtySession, PtySize, PtySpec, select_shell};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::DesktopCommandError;

/// Renderer-facing event names (spec §4).
pub(crate) const PTY_DATA_EVENT: &str = "pty://data";
pub(crate) const PTY_WRITE_UNAVAILABLE_EVENT: &str = "pty://write-unavailable";
pub(crate) const PTY_EXIT_EVENT: &str = "pty://exit";

const PTY_SPAWN_OPERATION: &str = "pty_spawn";
const PTY_WRITE_OPERATION: &str = "pty_write";
const PTY_RESIZE_OPERATION: &str = "pty_resize";
const PTY_KILL_OPERATION: &str = "pty_kill";
const PTY_HAS_PTY_OPERATION: &str = "pty_has_pty";
const PTY_ACK_DATA_OPERATION: &str = "pty_ack_data";
const PTY_GET_SIZE_OPERATION: &str = "pty_get_size";

/// Scrollback rows driving the byte owner's pending cap
/// (`max(2 MiB, rows × 120)` with rows clamped 1000…50000).
const DEFAULT_SCROLLBACK_ROWS: u32 = 1000;

// ─── Wire DTOs ──────────────────────────────────────────────────────────────

/// Renderer-facing spawn request. `program` omitted → the platform default
/// shell selection (`shellOverride` wins, per the crate's `select_shell`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PtySpawnRequest {
    program: Option<String>,
    arg_vec: Option<Vec<String>>,
    cwd: Option<String>,
    shell_override: Option<String>,
    cols: u16,
    rows: u16,
    env: Option<HashMap<String, String>>,
}

/// Renderer-facing spawn response: the opaque handle and the resolved shell.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PtySpawnResponse {
    id: u64,
    shell: String,
}

/// `pty://data` payload: the session id plus the byte owner's batch —
/// `OutputBatch` is already serde camelCase, so flattening keeps the wire
/// shape a faithful mirror of the crate's `take_batch` (`chunks`,
/// `dropped`, `restoreMarker`).
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PtyDataEvent {
    id: u64,
    #[serde(flatten)]
    batch: OutputBatch,
}

/// `pty://exit` payload (spec §4: `{id, code}`).
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyExitEvent {
    id: u64,
    code: i32,
}

/// `pty_get_size` response: the pty size as known by the kernel (applied
/// resizes included), mirroring the crate's `applied_size`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyAppliedSize {
    pub cols: u16,
    pub rows: u16,
}

fn exit_code_of(cause: &mcode_process::ExitCause) -> i32 {
    match cause {
        mcode_process::ExitCause::Exited { exit_code } => *exit_code,
        // node-pty discipline: a signalled death reports exit code 0.
        mcode_process::ExitCause::Signaled { .. } => 0,
        mcode_process::ExitCause::StopUnverified => -1,
    }
}

// ─── Managed state ──────────────────────────────────────────────────────────

struct NativePtyStateInner {
    sessions: Mutex<HashMap<u64, Arc<Mutex<PtySession>>>>,
    next_handle: AtomicU64,
    global_credit: Mutex<mcode_pty::GlobalCredit>,
}

impl std::fmt::Debug for NativePtyStateInner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let live = self
            .sessions
            .lock()
            .map(|sessions| sessions.len())
            .unwrap_or_default();
        formatter
            .debug_struct("NativePtyStateInner")
            .field("live_sessions", &live)
            .field("next_handle", &self.next_handle.load(Ordering::SeqCst))
            .finish()
    }
}

/// Tauri-managed PTY state shared by every `pty_*` command.
#[derive(Clone, Debug)]
pub struct NativePtyState(Arc<NativePtyStateInner>);

impl NativePtyState {
    pub fn new() -> Self {
        Self(Arc::new(NativePtyStateInner {
            sessions: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            global_credit: Mutex::new(mcode_pty::GlobalCredit::default()),
        }))
    }

    fn allocate_handle(&self) -> u64 {
        self.0.next_handle.fetch_add(1, Ordering::SeqCst)
    }

    fn insert(&self, id: u64, session: PtySession) -> Arc<Mutex<PtySession>> {
        let shared = Arc::new(Mutex::new(session));
        self.0
            .sessions
            .lock()
            .expect("pty sessions lock")
            .insert(id, Arc::clone(&shared));
        shared
    }

    fn remove(&self, id: u64) -> Option<Arc<Mutex<PtySession>>> {
        let session = self
            .0
            .sessions
            .lock()
            .expect("pty sessions lock")
            .remove(&id);
        if let Some(session) = &session {
            session
                .lock()
                .expect("pty session lock")
                .release_delivery_credit(
                    &mut self.0.global_credit.lock().expect("pty credit lock"),
                );
        }
        session
    }

    fn remove_instance(&self, id: u64, expected: &Arc<Mutex<PtySession>>) -> bool {
        let removed = {
            let mut sessions = self.0.sessions.lock().expect("pty sessions lock");
            if !sessions
                .get(&id)
                .is_some_and(|session| Arc::ptr_eq(session, expected))
            {
                return false;
            }
            sessions.remove(&id)
        };
        if let Some(session) = removed {
            session
                .lock()
                .expect("pty session lock")
                .release_delivery_credit(
                    &mut self.0.global_credit.lock().expect("pty credit lock"),
                );
        }
        true
    }

    pub(crate) fn get(&self, id: u64) -> Option<Arc<Mutex<PtySession>>> {
        self.0
            .sessions
            .lock()
            .expect("pty sessions lock")
            .get(&id)
            .cloned()
    }

    pub(crate) fn owns_pane(&self, pane_key: &str, pty_id: &str) -> bool {
        let Some(session) = pty_id.parse::<u64>().ok().and_then(|id| self.get(id)) else {
            return false;
        };
        session.lock().is_ok_and(|session| {
            let env = match &session.spec().env_policy {
                mcode_process::EnvironmentPolicy::Inherit { deltas, .. } => deltas,
                mcode_process::EnvironmentPolicy::Explicit { map, .. } => map,
            };
            !session.is_dead()
                && env
                    .iter()
                    .rev()
                    .find(|(key, _)| key == "MCODE_PANE_KEY")
                    .map(|(_, value)| value.as_str())
                    == Some(pane_key)
        })
    }

    fn ids(&self) -> Vec<u64> {
        self.0
            .sessions
            .lock()
            .expect("pty sessions lock")
            .keys()
            .copied()
            .collect()
    }
}

impl Default for NativePtyState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Error mapping ──────────────────────────────────────────────────────────

fn pty_error(error: PtyError, operation: &'static str) -> DesktopCommandError {
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
        PTY_SPAWN_OPERATION => "MCode could not start this terminal. Try again.",
        PTY_WRITE_OPERATION => "MCode could not send this input to the terminal. Try again.",
        PTY_RESIZE_OPERATION => "MCode could not resize this terminal. Try again.",
        PTY_KILL_OPERATION => "MCode could not close this terminal. Try again.",
        PTY_HAS_PTY_OPERATION => "MCode could not check this terminal's state. Try again.",
        PTY_ACK_DATA_OPERATION => "MCode could not update this terminal's progress. Try again.",
        PTY_GET_SIZE_OPERATION => "MCode could not read this terminal's size. Try again.",
        _ => "MCode could not complete the terminal request. Try again.",
    }
}

/// Parses the renderer's PTY id (the wire contract sends stringified ids
/// this wave). Numeric ids are allocated Rust-side only, so any non-numeric
/// id — which includes every `remote:`-prefixed runtime terminal handle and
/// the empty string — is invalid and must fail with `pty_invalid_id` BEFORE
/// any local-state lookup (grounding: the original throws
/// 'Invalid PTY provider id' for empty/`remote:` ids, pty.ts:7863-7867;
/// opaque string ids stay routing keys). A plain parse failure covers
/// `remote:` — it is non-numeric — so no mcode-ssh dependency is needed.
fn parse_pty_id(id: &str, operation: &'static str) -> Result<u64, DesktopCommandError> {
    id.parse::<u64>().map_err(|_| {
        command_error(
            "pty_invalid_id",
            operation,
            format!("invalid pty id: {id:?}"),
        )
    })
}

// ─── Spawn spec assembly ────────────────────────────────────────────────────

/// Builds the crate spec from the renderer request. With no `program`, the
/// crate's shell selection resolves the platform default and `shellOverride`
/// wins over the ladder (spec §6 override precedence). Explicit `env` entries
/// ride the inherit policy as deltas, so the crate's scrub list still applies.
fn build_spec(request: &PtySpawnRequest) -> Result<(PtySpec, String), PtyError> {
    let (program, args) = match &request.program {
        Some(program) => {
            if program.trim().is_empty() {
                return Err(PtyError::PtyInvalidArgument {
                    detail: "program must not be empty".to_string(),
                });
            }
            (program.clone(), request.arg_vec.clone().unwrap_or_default())
        }
        None => {
            let launch = select_shell(request.shell_override.as_deref());
            (launch.program, launch.args)
        }
    };

    let env_policy = match &request.env {
        Some(deltas) if deltas.is_empty() => mcode_process::EnvironmentPolicy::default(),
        Some(deltas) => mcode_process::EnvironmentPolicy::Inherit {
            deltas: deltas.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            removes: Vec::new(),
        },
        None => mcode_process::EnvironmentPolicy::default(),
    };

    let spec = PtySpec {
        program,
        args,
        cwd: request.cwd.clone(),
        env_policy,
        size: PtySize {
            cols: request.cols,
            rows: request.rows,
        },
        scrollback_rows: DEFAULT_SCROLLBACK_ROWS,
    };
    spec.validate()?;
    Ok((spec, resolved_shell_name(request)))
}

/// The shell identity reported back to the renderer: the explicit program or
/// override when present, otherwise what the default ladder picked.
fn resolved_shell_name(request: &PtySpawnRequest) -> String {
    request
        .program
        .clone()
        .or_else(|| request.shell_override.clone())
        .unwrap_or_else(|| select_shell(None).program)
}

// ─── Commands ───────────────────────────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_spawn(
    app: AppHandle,
    state: tauri::State<'_, NativePtyState>,
    mut spec: PtySpawnRequest,
) -> Result<PtySpawnResponse, DesktopCommandError> {
    let id = state.allocate_handle();
    // agentStatus hooks (buildPtyEnv, server.ts:2882-2900): when the renderer
    // pane identity is present (MCODE_PANE_KEY) and the hook server is
    // running, inject the loopback endpoint env so installed CLI hooks
    // attribute callbacks to this pane. Server-down ⇒ inject nothing — hooks
    // exit early on missing env, which is the original behavior.
    if spec
        .env
        .as_ref()
        .is_some_and(|env| env.contains_key("MCODE_PANE_KEY"))
        && let Some(deltas) = crate::agent_status_http::hook_env_deltas()
    {
        spec.env.get_or_insert_with(Default::default).extend(deltas);
    }
    let (pty_spec, shell) =
        build_spec(&spec).map_err(|error| pty_error(error, PTY_SPAWN_OPERATION))?;

    // Spawn off the async runtime: ConPTY/openpty setup blocks.
    let id_spawned = tauri::async_runtime::spawn_blocking(move || {
        PtySession::spawn(PtyHandle(id), pty_spec).map(|session| (id, session))
    })
    .await
    .map_err(|error| {
        command_error(
            "pty_spawn_failed",
            PTY_SPAWN_OPERATION,
            format!("pty spawn task failed: {error}"),
        )
    })?
    .map_err(|error| pty_error(error, PTY_SPAWN_OPERATION))?;

    let (id, session) = id_spawned;
    let shared = state.insert(id, session);
    spawn_pty_forwarder(app, id, shared, state.inner().clone());

    Ok(PtySpawnResponse { id, shell })
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_write(
    app: tauri::AppHandle,
    state: tauri::State<'_, NativePtyState>,
    id: String,
    data: String,
) -> Result<(), DesktopCommandError> {
    pty_write_for(&app, &state, id, data).await
}

/// `pty_write` body without the Tauri runtime: writes to the session and,
/// on every failure path, emits `pty://write-unavailable` exactly once
/// (spec §1 `onWriteUnavailable` — the renderer's flow-control recovery
/// arm). The payload carries the decimal-string id and the failure reason;
/// emit failures are logged and never mask the original command error.
/// Generic over the runtime like `ssh_open_pty_for` so the mock-app tests
/// can drive it.
pub async fn pty_write_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &NativePtyState,
    id: String,
    data: String,
) -> Result<(), DesktopCommandError> {
    let id = parse_pty_id(&id, PTY_WRITE_OPERATION)?;
    let session = state.get(id).ok_or_else(|| {
        emit_write_unavailable(app, &id.to_string(), "pty session is gone");
        command_error(
            "pty_session_gone",
            PTY_WRITE_OPERATION,
            "pty session is gone",
        )
    })?;
    let write_result = tauri::async_runtime::spawn_blocking(move || {
        session
            .lock()
            .expect("pty session lock")
            .write_input(data.as_bytes())
    })
    .await;
    match write_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            emit_write_unavailable(app, &id.to_string(), &error.to_string());
            Err(pty_error(error, PTY_WRITE_OPERATION))
        }
        Err(error) => {
            emit_write_unavailable(
                app,
                &id.to_string(),
                &format!("pty write task failed: {error}"),
            );
            Err(command_error(
                "pty_session_gone",
                PTY_WRITE_OPERATION,
                format!("pty write task failed: {error}"),
            ))
        }
    }
}

/// Announces `pty://write-unavailable` exactly once per failed `pty_write`:
/// the renderer's flow-control recovery arm (`TERMINAL_UI_INTEGRATION.md` §1
/// `onWriteUnavailable`). The payload carries the decimal-string id and the
/// failure reason; emit failures are logged and never mask the command error.
fn emit_write_unavailable<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pty_id: &str,
    reason: &str,
) {
    let payload = serde_json::json!({
        "ptyId": pty_id,
        "reason": reason,
    });
    if let Err(error) = app.emit(PTY_WRITE_UNAVAILABLE_EVENT, payload) {
        eprintln!("[pty] failed to emit write-unavailable for {pty_id}: {error}");
    }
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_resize(
    state: tauri::State<'_, NativePtyState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), DesktopCommandError> {
    let id = parse_pty_id(&id, PTY_RESIZE_OPERATION)?;
    let session = state.get(id).ok_or_else(|| {
        command_error(
            "pty_session_gone",
            PTY_RESIZE_OPERATION,
            "pty session is gone",
        )
    })?;
    // Coalesced: the reader loop applies the latest requested size.
    session.lock().expect("pty session lock").resize(cols, rows);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_kill(
    state: tauri::State<'_, NativePtyState>,
    id: String,
) -> Result<(), DesktopCommandError> {
    let id = parse_pty_id(&id, PTY_KILL_OPERATION)?;
    // Idempotent: an unknown id already exited (spec §8 kill).
    let Some(session) = state.remove(id) else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = session.lock().expect("pty session lock");
        // Closing the pane abandons queued delivery; stop the owned process immediately.
        let _ = guard.kill();
    })
    .await
    .map_err(|error| {
        command_error(
            "pty_session_gone",
            PTY_KILL_OPERATION,
            format!("pty kill task failed: {error}"),
        )
    })
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_has_pty(
    state: tauri::State<'_, NativePtyState>,
    id: String,
) -> Result<bool, DesktopCommandError> {
    let id = parse_pty_id(&id, PTY_HAS_PTY_OPERATION)?;
    Ok(state.get(id).is_some())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_ack_data(
    state: tauri::State<'_, NativePtyState>,
    id: String,
    processed_chars: u64,
) -> Result<(), DesktopCommandError> {
    let id = parse_pty_id(&id, PTY_ACK_DATA_OPERATION)?;
    pty_ack_data_for(&state, id, processed_chars)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_get_size(
    state: tauri::State<'_, NativePtyState>,
    id: String,
) -> Result<Option<PtyAppliedSize>, DesktopCommandError> {
    let id = parse_pty_id(&id, PTY_GET_SIZE_OPERATION)?;
    pty_get_size_for(&state, id)
}

// ─── Command seams (renderer-contract tests) ────────────────────────────────

/// `pty_ack_data` body without the Tauri runtime: registers the renderer's
/// processed-char watermark on the byte owner (spec §5.3 cumulative max-merge
/// ACK). Unknown ids report `pty_session_gone`, matching the command.
pub fn pty_ack_data_for(
    state: &NativePtyState,
    id: u64,
    processed_chars: u64,
) -> Result<(), DesktopCommandError> {
    let session = state.get(id).ok_or_else(|| {
        command_error(
            "pty_session_gone",
            PTY_ACK_DATA_OPERATION,
            "pty session is gone",
        )
    })?;
    session.lock().expect("pty session lock").ack_with_global(
        &mut state.0.global_credit.lock().expect("pty credit lock"),
        processed_chars,
    );
    Ok(())
}

/// `pty_get_size` body without the Tauri runtime: the applied kernel size for
/// the session, or `None` when the pty has no size yet. Unknown ids report
/// `pty_session_gone`, matching the command.
pub fn pty_get_size_for(
    state: &NativePtyState,
    id: u64,
) -> Result<Option<PtyAppliedSize>, DesktopCommandError> {
    let session = state.get(id).ok_or_else(|| {
        command_error(
            "pty_session_gone",
            PTY_GET_SIZE_OPERATION,
            "pty session is gone",
        )
    })?;
    Ok(session
        .lock()
        .expect("pty session lock")
        .applied_size()
        .map(|(cols, rows)| PtyAppliedSize { cols, rows }))
}

/// Harness registration for the seams: inserts an already-spawned session
/// under an explicit id (production spawn registers through `pty_spawn`).
/// Returns the shared session handle so tests can drive the session directly.
pub fn pty_register_session_for(
    state: &NativePtyState,
    id: u64,
    session: PtySession,
) -> Arc<Mutex<PtySession>> {
    state.insert(id, session)
}

// ─── Reader forwarding ──────────────────────────────────────────────────────

/// One thread per session: drains `take_batch` — the crate's bounded byte
/// owner is the only queue, so nothing here grows unboundedly — and emits
/// `pty://data`. On death it drains one final flush and announces `pty://exit`
/// exactly once, then the session's last reference drops (killing the child).
fn spawn_pty_forwarder<R: tauri::Runtime>(
    app: AppHandle<R>,
    id: u64,
    session: Arc<Mutex<PtySession>>,
    state: NativePtyState,
) {
    std::thread::Builder::new()
        .name(format!("mcode-pty-forwarder-{id}"))
        .spawn(move || {
            let mut exit_code = None;
            loop {
                if !state
                    .get(id)
                    .is_some_and(|current| Arc::ptr_eq(&current, &session))
                {
                    return;
                }
                let batch = {
                    let mut guard = session.lock().expect("pty session lock");
                    if !state
                        .get(id)
                        .is_some_and(|current| Arc::ptr_eq(&current, &session))
                    {
                        return;
                    }
                    if exit_code.is_none() && guard.child_exited() {
                        exit_code = Some(exit_code_of(&guard.stop_reader_after_child_exit()));
                    }
                    guard.reserve_batch(&mut state.0.global_credit.lock().expect("pty credit lock"))
                };
                if !batch.is_empty() {
                    if !state
                        .get(id)
                        .is_some_and(|current| Arc::ptr_eq(&current, &session))
                    {
                        return;
                    }
                    let event = PtyDataEvent { id, batch };
                    if let Err(error) = app.emit(PTY_DATA_EVENT, &event) {
                        session.lock().expect("pty session lock").rollback_batch(
                            &mut state.0.global_credit.lock().expect("pty credit lock"),
                            event.batch,
                        );
                        eprintln!("[pty] failed to emit data for {id}: {error}");
                    }
                }
                if let Some(code) = exit_code
                    && !session
                        .lock()
                        .expect("pty session lock")
                        .has_pending_delivery()
                {
                    if !state.remove_instance(id, &session) {
                        return;
                    }
                    if let Err(error) = app.emit(PTY_EXIT_EVENT, PtyExitEvent { id, code }) {
                        eprintln!("[pty] failed to emit exit for {id}: {error}");
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
        .expect("failed to spawn pty forwarder");
}

// ─── Shutdown ───────────────────────────────────────────────────────────────

/// Shutdown abandons renderer delivery credit and stops each owned session.
pub(crate) fn kill_all_sessions_for_shutdown(state: &NativePtyState) {
    for id in state.ids() {
        if let Some(session) = state.remove(id) {
            let mut guard = session.lock().expect("pty session lock");
            let _ = guard.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tauri::Listener;

    #[test]
    fn failed_write_emits_write_unavailable_once_with_decimal_string_id() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let state = NativePtyState::new();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let sink = std::sync::Arc::clone(&events);
        let _subscription = handle.listen(PTY_WRITE_UNAVAILABLE_EVENT, move |event| {
            sink.lock()
                .expect("write-unavailable event lock")
                .push(serde_json::from_str(event.payload()).expect("event payload json"));
        });

        // No session registered under this id: the write fails with
        // `pty_session_gone`, the deterministic error path.
        let gone_id: u64 = 4242;
        assert!(state.get(gone_id).is_none());

        // The command body below the parse seam is what emits; drive the
        // runtime-generic seam so the mock-app handle is the real emitter.
        let result = tauri::async_runtime::block_on(super::pty_write_for(
            &handle,
            &state,
            gone_id.to_string(),
            "echo hi\n".to_owned(),
        ));
        assert!(result.is_err());

        let emitted = events.lock().expect("write-unavailable event lock").clone();
        assert_eq!(emitted.len(), 1, "a failed write emits exactly once");
        let payload = &emitted[0];
        assert_eq!(payload["ptyId"], Value::String(gone_id.to_string()));
        assert!(payload["reason"].is_string());
        assert_eq!(payload.as_object().map(|object| object.len()), Some(2));
    }

    #[test]
    fn successful_write_emits_nothing() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let state = NativePtyState::new();

        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let sink = std::sync::Arc::clone(&events);
        let _subscription = handle.listen(PTY_WRITE_UNAVAILABLE_EVENT, move |event| {
            sink.lock()
                .expect("write-unavailable event lock")
                .push(serde_json::from_str(event.payload()).expect("event payload json"));
        });

        let session = mcode_pty::PtySession::spawn(
            mcode_pty::PtyHandle(901),
            mcode_pty::PtySpec {
                program: "cmd".to_owned(),
                args: vec!["/c".to_owned(), "pause".to_owned()],
                cwd: None,
                env_policy: mcode_process::EnvironmentPolicy::Inherit {
                    deltas: Vec::new(),
                    removes: Vec::new(),
                },
                size: mcode_pty::PtySize::default(),
                scrollback_rows: 1000,
            },
        )
        .expect("spawn a live session for the happy path");
        pty_register_session_for(&state, 901, session);

        let result = tauri::async_runtime::block_on(super::pty_write_for(
            &handle,
            &state,
            "901".to_owned(),
            "echo hi\n".to_owned(),
        ));
        assert!(result.is_ok());

        let emitted = events.lock().expect("write-unavailable event lock").clone();
        assert!(emitted.is_empty(), "a successful write emits nothing");
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "subprocess output fixture"]
    fn credit_forwarder_child_output() {
        use std::io::Write;
        std::io::stdout()
            .write_all(&vec![b'x'; 1024 * 1024])
            .unwrap();
        std::io::stdout().flush().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn production_forwarder_obeys_credit_and_delivers_exit_after_final_ack() {
        let app = tauri::test::mock_app();
        let state = NativePtyState::new();
        let received = Arc::new(AtomicU64::new(0));
        let acknowledge = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let id = 902;
        let output_count = Arc::clone(&received);
        let ack_enabled = Arc::clone(&acknowledge);
        let ack_state = state.clone();
        app.listen(PTY_DATA_EVENT, move |event| {
            let value: Value = serde_json::from_str(event.payload()).unwrap();
            if value["id"] != id {
                return;
            }
            let batch: OutputBatch = serde_json::from_value(value).unwrap();
            let total = output_count.fetch_add(batch.delivered_units(), Ordering::SeqCst)
                + batch.delivered_units();
            if ack_enabled.load(Ordering::SeqCst) {
                pty_ack_data_for(&ack_state, id, total).unwrap();
            }
        });
        let (sender, receiver) = std::sync::mpsc::channel();
        app.listen(PTY_EXIT_EVENT, move |_| {
            let _ = sender.send(());
        });
        let session = PtySession::spawn(
            PtyHandle(id),
            PtySpec {
                program: std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                args: vec![
                    "--exact".into(),
                    "pty_commands::tests::credit_forwarder_child_output".into(),
                    "--ignored".into(),
                    "--nocapture".into(),
                ],
                cwd: None,
                env_policy: Default::default(),
                size: PtySize::default(),
                scrollback_rows: 1000,
            },
        )
        .unwrap();
        let shared = state.insert(id, session);
        spawn_pty_forwarder(app.handle().clone(), id, shared, state.clone());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while received.load(Ordering::SeqCst)
            < mcode_pty::byte_owner::PER_PTY_INFLIGHT_CAP_CHARS as u64
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let stalled_at = received.load(Ordering::SeqCst);
        assert!(stalled_at > 0);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(received.load(Ordering::SeqCst), stalled_at);
        assert!(
            state.0.global_credit.lock().unwrap().in_flight()
                <= mcode_pty::byte_owner::PER_PTY_INFLIGHT_CAP_CHARS as u64
        );
        acknowledge.store(true, Ordering::SeqCst);
        pty_ack_data_for(&state, id, received.load(Ordering::SeqCst)).unwrap();
        receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("final ACK must allow exit");
        assert!(received.load(Ordering::SeqCst) >= 1024 * 1024);
        assert!(state.get(id).is_none());
        assert_eq!(state.0.global_credit.lock().unwrap().in_flight(), 0);
    }
}
