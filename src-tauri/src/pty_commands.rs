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
pub(crate) const PTY_EXIT_EVENT: &str = "pty://exit";

/// Graceful-stop ladder bound before a forced kill (spec §8: bounded kill).
const KILL_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

const PTY_SPAWN_OPERATION: &str = "pty_spawn";
const PTY_WRITE_OPERATION: &str = "pty_write";
const PTY_RESIZE_OPERATION: &str = "pty_resize";
const PTY_KILL_OPERATION: &str = "pty_kill";
const PTY_HAS_PTY_OPERATION: &str = "pty_has_pty";

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
pub(crate) struct NativePtyState(Arc<NativePtyStateInner>);

impl NativePtyState {
    pub(crate) fn new() -> Self {
        Self(Arc::new(NativePtyStateInner {
            sessions: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
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
        self.0
            .sessions
            .lock()
            .expect("pty sessions lock")
            .remove(&id)
    }

    fn get(&self, id: u64) -> Option<Arc<Mutex<PtySession>>> {
        self.0
            .sessions
            .lock()
            .expect("pty sessions lock")
            .get(&id)
            .cloned()
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
        _ => "MCode could not complete the terminal request. Try again.",
    }
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
    spec: PtySpawnRequest,
) -> Result<PtySpawnResponse, DesktopCommandError> {
    let id = state.allocate_handle();
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
    spawn_pty_forwarder(app, id, shared);

    Ok(PtySpawnResponse { id, shell })
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_write(
    state: tauri::State<'_, NativePtyState>,
    id: u64,
    data: String,
) -> Result<(), DesktopCommandError> {
    let session = state.get(id).ok_or_else(|| {
        command_error(
            "pty_session_gone",
            PTY_WRITE_OPERATION,
            "pty session is gone",
        )
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        session
            .lock()
            .expect("pty session lock")
            .write_input(data.as_bytes())
    })
    .await
    .map_err(|error| {
        command_error(
            "pty_session_gone",
            PTY_WRITE_OPERATION,
            format!("pty write task failed: {error}"),
        )
    })?
    .map_err(|error| pty_error(error, PTY_WRITE_OPERATION))
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn pty_resize(
    state: tauri::State<'_, NativePtyState>,
    id: u64,
    cols: u16,
    rows: u16,
) -> Result<(), DesktopCommandError> {
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
    id: u64,
) -> Result<(), DesktopCommandError> {
    // Idempotent: an unknown id already exited (spec §8 kill).
    let Some(session) = state.remove(id) else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = session.lock().expect("pty session lock");
        // Graceful ladder: bounded flush-wait, then force-kill. The
        // forwarder owns the exit announcement, so this only guarantees the
        // child is gone.
        let _ = guard.wait_with_flush(KILL_FLUSH_TIMEOUT);
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
    id: u64,
) -> Result<bool, DesktopCommandError> {
    Ok(state.get(id).is_some())
}

// ─── Reader forwarding ──────────────────────────────────────────────────────

/// One thread per session: drains `take_batch` — the crate's bounded byte
/// owner is the only queue, so nothing here grows unboundedly — and emits
/// `pty://data`. On death it drains one final flush and announces `pty://exit`
/// exactly once, then the session's last reference drops (killing the child).
fn spawn_pty_forwarder(app: AppHandle, id: u64, session: Arc<Mutex<PtySession>>) {
    std::thread::Builder::new()
        .name(format!("mcode-pty-forwarder-{id}"))
        .spawn(move || {
            loop {
                let (batch, dead) = {
                    let guard = session.lock().expect("pty session lock");
                    (guard.take_batch(), guard.is_dead())
                };
                if !batch.chunks.is_empty()
                    || batch.dropped.is_some()
                    || batch.restore_marker.is_some()
                {
                    let event = PtyDataEvent { id, batch };
                    if let Err(error) = app.emit(PTY_DATA_EVENT, &event) {
                        eprintln!("[pty] failed to emit data event for {id}: {error}");
                    }
                }
                if dead {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }

            // Flush-before-exit: whatever the reader captured before death
            // must reach the renderer before the exit announcement.
            let exit_code = {
                let mut guard = session.lock().expect("pty session lock");
                match guard.wait_with_flush(Duration::from_secs(1)) {
                    Ok((batch, cause)) => {
                        if !batch.chunks.is_empty() {
                            let event = PtyDataEvent { id, batch };
                            if let Err(error) = app.emit(PTY_DATA_EVENT, &event) {
                                eprintln!("[pty] failed to emit exit flush for {id}: {error}");
                            }
                        }
                        exit_code_of(&cause)
                    }
                    Err(error) => {
                        eprintln!("[pty] exit wait failed for {id}: {error}");
                        -1
                    }
                }
            };
            if let Err(error) = app.emit(
                PTY_EXIT_EVENT,
                PtyExitEvent {
                    id,
                    code: exit_code,
                },
            ) {
                eprintln!("[pty] failed to emit exit event for {id}: {error}");
            }
        })
        .expect("failed to spawn pty forwarder");
}

// ─── Shutdown ───────────────────────────────────────────────────────────────

/// Kills every live session: bounded flush-wait per session, then force.
/// Called from the app shutdown hook; the forwarders observe the deaths and
/// announce `pty://exit` on their own threads.
pub(crate) fn kill_all_sessions_for_shutdown(state: &NativePtyState) {
    for id in state.ids() {
        if let Some(session) = state.remove(id) {
            let mut guard = session.lock().expect("pty session lock");
            let _ = guard.wait_with_flush(KILL_FLUSH_TIMEOUT);
            let _ = guard.kill();
        }
    }
}
