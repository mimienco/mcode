//! agentStatus hook HTTP server — the Tauri equivalent of the loopback
//! listener in `src/main/agent-hooks/server.ts:2402-2536` (start/handleRequest)
//! and `buildPtyEnv` (`:2882-2900`). Serves POST /hook/<source> for the CLIs'
//! installed hook scripts; ingests into the shared [`agent_status_store`];
//! fans every store effect out to the renderer as `agentStatus:*` events.
//!
//! Ported invariants: ephemeral port on 127.0.0.1, random per-process token,
//! token check → 403, unknown route → 404, malformed → 204 fail-open,
//! endpoint file written on listen, PTY env deltas published only while the
//! server runs (`buildPtyEnv` returns {} before start).
//!
//! Store-effect fanout is push-based: `apply_enriched` results map directly to
//! `agentStatus:set`; the clear/registry effects are produced by the commands
//! module and emitted there.

use std::collections::HashMap;

use parking_lot::Mutex as ParkingMutex;
use tauri::{AppHandle, Emitter, Manager};

use mcode_agent_status::endpoint::{EndpointFileFields, write_endpoint_file};
use mcode_agent_status::http::{HookRequestParts, HookServerConfig, handle_hook_request};
use mcode_agent_status::store::{StatusStore, StoreEffect, StoreHandle, epoch_now};
use mcode_agent_status::types::MCODE_HOOK_PROTOCOL_VERSION;

use crate::DesktopCommandError;

fn agent_status_http_error(
    code: &'static str,
    operation: &'static str,
    message: impl Into<String>,
) -> DesktopCommandError {
    DesktopCommandError::new(code, operation, message.into(), "", None, None)
}

/// Renderer event names (preload contract; useIpcEvents.ts subscribes).
pub const AGENT_STATUS_SET_EVENT: &str = "agentStatus:set";
pub const AGENT_STATUS_CLEAR_EVENT: &str = "agentStatus:clear";

/// Process-global status store shared by the commands and the HTTP task.
/// Runtime input (the resolved data dir) arrives at server start via
/// `attach_persist_dir`, so this is a OnceLock-equivalent LazyLock with a
/// dir-less first construction and an explicit attach step.
pub fn agent_status_store() -> StoreHandle {
    static STORE: std::sync::LazyLock<StoreHandle> = std::sync::LazyLock::new(|| {
        std::sync::Arc::new(std::sync::Mutex::new(StatusStore::new(None)))
    });
    STORE.clone()
}

/// Flush the last timer window before the runtime exits.
pub fn flush_agent_status() -> std::io::Result<()> {
    let store = agent_status_store();
    let mut guard = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.persist_if_dirty().map(|_| ())
}

struct HookServerState {
    port: u16,
    token: String,
    env: String,
    endpoint_file_written: bool,
    endpoint_file_path: Option<String>,
}

static HOOK_SERVER_STATE: ParkingMutex<Option<HookServerState>> = ParkingMutex::new(None);

/// `buildPtyEnv()` (`server.ts:2882-2900`): None while the server is down —
/// hooks exit early on missing env (hook-stdin-contract.ts:47-51), which is
/// the original behavior.
pub fn hook_env_deltas() -> Option<HashMap<String, String>> {
    let snapshot = {
        let guard = HOOK_SERVER_STATE.lock();
        let state = guard.as_ref()?;
        (
            state.port,
            state.token.clone(),
            state.env.clone(),
            state.endpoint_file_written,
            state.endpoint_file_path.clone(),
        )
    };
    let (port, token, env_value, endpoint_file_written, endpoint_file_path) = snapshot;
    let mut env = HashMap::new();
    env.insert("MCODE_AGENT_HOOK_PORT".to_string(), port.to_string());
    env.insert("MCODE_AGENT_HOOK_TOKEN".to_string(), token);
    env.insert("MCODE_AGENT_HOOK_ENV".to_string(), env_value);
    env.insert(
        "MCODE_AGENT_HOOK_VERSION".to_string(),
        MCODE_HOOK_PROTOCOL_VERSION.to_string(),
    );
    // Why: hooks source this file at invocation (`server.ts:2893-2896`).
    if endpoint_file_written && let Some(path) = endpoint_file_path {
        env.insert("MCODE_AGENT_HOOK_ENDPOINT".to_string(), path);
    }
    Some(env)
}

/// The endpoint credential must come from the operating system CSPRNG.
fn generate_token() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Resolve userData/agent-hooks (MCODE_TAURI_SETTINGS_PATH redirect honored
/// for parity with settings_path()).
fn hook_data_dir_with_app(app: &AppHandle) -> Result<std::path::PathBuf, DesktopCommandError> {
    if let Some(path) = std::env::var_os("MCODE_TAURI_SETTINGS_PATH") {
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            return Ok(parent.join("agent-hooks"));
        }
    }
    let user_data = app.path().app_data_dir().map_err(|error| {
        agent_status_http_error(
            "agent_status_user_data_unavailable",
            "start_agent_status_hook_server",
            format!("app data dir unavailable: {error}"),
        )
    })?;
    Ok(user_data.join("agent-hooks"))
}

/// Start the loopback hook server. Failure is logged, not fatal — a host
/// without hooks is degraded, not broken (matching runtime RPC handling).
pub fn start_agent_status_hook_server(app: AppHandle) -> Result<(), DesktopCommandError> {
    let data_dir = hook_data_dir_with_app(&app)?;
    let store = agent_status_store();
    {
        let mut guard = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.attach_persist_dir(data_dir.clone());
    }

    let token = generate_token().map_err(|error| {
        agent_status_http_error(
            "agent_status_entropy_failed",
            "start_agent_status_hook_server",
            error.to_string(),
        )
    })?;
    let env = if cfg!(debug_assertions) {
        "development"
    } else {
        "production"
    }
    .to_string();

    let std_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|error| {
        agent_status_http_error(
            "agent_status_bind_failed",
            "start_agent_status_hook_server",
            format!("failed to bind loopback hook server: {error}"),
        )
    })?;
    let port = std_listener
        .local_addr()
        .map_err(|error| {
            agent_status_http_error(
                "agent_status_bind_failed",
                "start_agent_status_hook_server",
                format!("failed to read local addr: {error}"),
            )
        })?
        .port();

    // Publish the endpoint file + env deltas BEFORE accepting connections
    // (metadata-first discipline, server.ts:2512-2536).
    let endpoint_fields = EndpointFileFields {
        port,
        token: token.clone(),
        env: env.clone(),
        version: MCODE_HOOK_PROTOCOL_VERSION.to_string(),
    };
    let endpoint_path = match write_endpoint_file(&data_dir, &endpoint_fields) {
        Ok(path) => Some(path),
        Err(error) => {
            eprintln!("[agent-hooks] endpoint publication failed; using PTY environment: {error}");
            None
        }
    };
    {
        let mut guard = HOOK_SERVER_STATE.lock();
        *guard = Some(HookServerState {
            port,
            token: token.clone(),
            env: env.clone(),
            endpoint_file_written: endpoint_path.as_ref().is_some_and(|path| path.is_file()),
            endpoint_file_path: endpoint_path.map(|path| path.to_string_lossy().into_owned()),
        });
    }

    let config = HookServerConfig { token, env };
    let app_handle = app.clone();
    std::thread::Builder::new()
        .name("agent-status-hooks".into())
        .spawn(move || {
            if let Err(error) = run_accept_loop(std_listener, config, app_handle) {
                eprintln!("[agent-hooks] hook server stopped: {error}");
            }
            *HOOK_SERVER_STATE.lock() = None;
        })
        .map_err(|error| {
            *HOOK_SERVER_STATE.lock() = None;
            agent_status_http_error(
                "agent_status_spawn_failed",
                "start_agent_status_hook_server",
                format!("failed to spawn hook server thread: {error}"),
            )
        })?;

    std::thread::Builder::new()
        .name("agent-status-persist".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(
                    mcode_agent_status::types::STATUS_PERSIST_DEBOUNCE_MS,
                ));
                if HOOK_SERVER_STATE.lock().is_none() {
                    break;
                }
                let mut guard = store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Err(error) = guard.persist_if_dirty() {
                    eprintln!("[agent-hooks] status persistence failed: {error}");
                }
            }
        })
        .map_err(|error| {
            agent_status_http_error(
                "agent_status_persist_spawn_failed",
                "start_agent_status_hook_server",
                error.to_string(),
            )
        })?;

    eprintln!("[agent-hooks] hook server listening on 127.0.0.1:{port}");
    Ok(())
}

/// One blocking thread; hook posts are tiny and rare (per tool-use boundary),
/// but concurrency is preserved via a small thread-per-connection spawn.
fn run_accept_loop(
    listener: std::net::TcpListener,
    config: HookServerConfig,
    app: AppHandle,
) -> std::io::Result<()> {
    let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming() {
        let stream = stream?;
        if active.fetch_add(1, std::sync::atomic::Ordering::AcqRel) >= 32 {
            active.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            continue;
        }
        let permit = ConnectionPermit(active.clone());
        let config = HookServerConfig {
            token: config.token.clone(),
            env: config.env.clone(),
        };
        let app = app.clone();
        let _ = std::thread::Builder::new()
            .name("agent-status-conn".into())
            .spawn(move || {
                let _permit = permit;
                let _ = serve_connection(app, config, stream);
            });
    }
    Ok(())
}

struct ConnectionPermit(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Minimal HTTP/1.1 read+write: exactly one request per connection (curl posts
/// with Connection: close semantics on this path; hyper is unnecessary for a
/// 4-line reply and keeps the response budget deterministic).
fn serve_connection(
    app: AppHandle,
    config: HookServerConfig,
    mut stream: std::net::TcpStream,
) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(
        5_000, // HOOK_REQUEST_SLOWLORIS_MS
    )));

    let request = match read_request_head_and_body(&mut stream) {
        Some(request) => request,
        None => {
            write_status(&mut stream, 404)?;
            return Ok(());
        }
    };
    let handle = agent_status_store();
    let mut store = handle
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (response, effects) = handle_hook_request(&mut store, &config, request, epoch_now());
    if matches!(
        response.statusline,
        Some(mcode_agent_status::http::StatusLineOutcome::AwaitingTrustedAccountConsumer(_))
    ) {
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "[agent-hooks] Claude statusline parsed; selected-account rate-limit consumer is not yet available"
            );
        }
    }
    emit_store_effects(&app, &effects);
    drop(store);
    let status = response.status;
    write_status(&mut stream, status)?;
    Ok(())
}

pub(crate) fn emit_store_effects(app: &AppHandle, effects: &[StoreEffect]) {
    for effect in effects {
        emit_store_effect(app, effect);
    }
}

pub(crate) fn emit_store_effect(app: &AppHandle, effect: &StoreEffect) {
    let (event, value) = match effect {
        StoreEffect::Set(payload) => (AGENT_STATUS_SET_EVENT, serde_json::to_value(payload)),
        StoreEffect::Clear(payload) => (AGENT_STATUS_CLEAR_EVENT, serde_json::to_value(payload)),
        StoreEffect::MigrationUnsupported(entry) => (
            "agentStatus:migrationUnsupported",
            serde_json::to_value(entry),
        ),
        StoreEffect::MigrationUnsupportedClear { pty_id } => (
            "agentStatus:migrationUnsupportedClear",
            serde_json::to_value(serde_json::json!({ "ptyId": pty_id })),
        ),
    };
    if let Ok(value) = value {
        let _ = app.emit(event, value);
    }
}

/// Read the request head, then the body honoring Content-Length (capped at
/// HOOK_REQUEST_MAX_BYTES + slack so oversized bodies are detected, not hung).
fn read_request_head_and_body(stream: &mut std::net::TcpStream) -> Option<HookRequestParts> {
    use std::io::Read;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let head_end;
    loop {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        stream.set_read_timeout(Some(remaining)).ok()?;
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_double_crlf(&buffer) {
            if position > 64 * 1024 {
                return None;
            }
            head_end = position;
            break;
        }
        if buffer.len() > 64 * 1024 {
            return None;
        }
    }
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next()?.trim().to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    if !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) || parts.next().is_some() {
        return None;
    }
    let mut content_length = None;
    let mut token_header = None;
    let mut content_type = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => {
                if content_length.is_some() {
                    return None;
                }
                content_length = Some(value.parse::<usize>().ok()?);
            }
            "transfer-encoding" => return None,
            "x-mcode-agent-hook-token" => token_header = Some(value.to_string()),
            "content-type" => content_type = Some(value.to_string()),
            _ => {}
        }
    }
    let content_length = content_length.unwrap_or(0);
    if content_length > mcode_agent_status::types::HOOK_REQUEST_MAX_BYTES + 1 {
        // Cap-exceeded: read nothing further; handler classifies via empty body.
        return Some(HookRequestParts {
            method,
            path,
            token_header,
            content_type,
            body: Vec::new(),
        });
    }
    let mut body = buffer[head_end + 4..].to_vec();
    let mut chunk = [0u8; 8192];
    while body.len() < content_length {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        stream.set_read_timeout(Some(remaining)).ok()?;
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        body.extend_from_slice(&chunk[..read.min(content_length - body.len())]);
    }
    body.truncate(content_length.min(mcode_agent_status::types::HOOK_REQUEST_MAX_BYTES + 1));
    Some(HookRequestParts {
        method,
        path,
        token_header,
        content_type,
        body,
    })
}

fn find_double_crlf(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_status(stream: &mut std::net::TcpStream, status: u16) -> std::io::Result<()> {
    let reason = match status {
        204 => "No Content",
        403 => "Forbidden",
        _ => "Not Found",
    };
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    let response =
        format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    std::io::Write::write_all(stream, response.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_wire(raw: &'static [u8]) -> Option<HookRequestParts> {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            stream.write_all(raw).unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let result = read_request_head_and_body(&mut stream);
        client.join().unwrap();
        result
    }

    #[test]
    fn truncated_and_ambiguous_http_bodies_are_rejected() {
        assert!(
            parse_wire(b"POST /hook/claude HTTP/1.1\r\nContent-Length: 20\r\n\r\nshort").is_none()
        );
        assert!(
            parse_wire(b"POST /hook/claude HTTP/1.1\r\nContent-Length: invalid\r\n\r\n").is_none()
        );
        assert!(
            parse_wire(
                b"POST /hook/claude HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nxx"
            )
            .is_none()
        );
        assert!(
            parse_wire(
                b"POST /hook/claude HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            )
            .is_none()
        );
    }

    #[test]
    fn env_deltas_absent_before_start() {
        // Cannot assert globally (other tests may have started the server);
        // the semantic contract is: Some while running, None before start.
        let is_stopped = HOOK_SERVER_STATE.lock().is_none();
        if is_stopped {
            assert!(hook_env_deltas().is_none());
        }
    }

    #[test]
    fn token_is_sixty_four_hex() {
        let token = generate_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn request_head_parser() {
        use std::io::Write;
        let raw = b"POST /hook/claude HTTP/1.1\r\nContent-Length: 5\r\nX-MCode-Agent-Hook-Token: tok\r\n\r\nhello";
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            stream.write_all(raw).unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        client.join().unwrap();
        let request = read_request_head_and_body(&mut stream).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/hook/claude");
        assert_eq!(request.token_header.as_deref(), Some("tok"));
        assert_eq!(request.body, b"hello");
    }

    #[test]
    fn renderer_event_names_match_preload_contract() {
        assert_eq!(AGENT_STATUS_SET_EVENT, "agentStatus:set");
        assert_eq!(AGENT_STATUS_CLEAR_EVENT, "agentStatus:clear");
    }
}
