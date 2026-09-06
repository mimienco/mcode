//! Headless-host runtime RPC plane — Rust port of the newline-JSON CLI
//! transport of `src/main/runtime/runtime-rpc.ts` (the Electron main host).
//!
//! # Which plane this is
//! This module serves the **framed CLI bootstrap plane**: a local (POSIX unix
//! socket / Windows named pipe) endpoint advertised via `mcode-runtime.json`,
//! speaking newline-delimited JSON envelopes guarded by a shared `authToken`.
//! It is the plane `src/cli/runtime/transport.ts` (the `mcode` CLI) dials with
//! `{id, authToken, method, params}` request lines. It is NOT the framed
//! binary relay plane (`src/main/ssh/relay-protocol.ts`, length-prefixed
//! frames with `MAX_MESSAGE_SIZE`); `agent_exec` routing does not flow over
//! here. No pairing/WebSocket transport either — that is the `enableWebSocket`
//! supplementary state in the TS host, out of scope for the headless host.
//!
//! # Protocol (must stay in lockstep with the TS readers)
//! Request line: `{id, authToken, method, params}`.
//! Reply frames (validated by `RuntimeRpcEnvelopeSchema`):
//!   success: `{"id":…,"ok":true,"result":…,"_meta":{"runtimeId":…}}`
//!   failure: `{"id":…,"ok":false,"error":{"code":…,"message":…},"_meta":{"runtimeId":null}}`
//! The CLI treats a `_meta.runtimeId` mismatch as `runtime_unavailable`
//! (src/cli/runtime/transport.ts:168), so `_meta` is always stamped.
//!
//! # Metadata-first discipline (runtime-rpc.ts:1207-1222)
//! The endpoint is bound and accepting, but the metadata file — the CLI's only
//! pointer at a live runtime — is written BEFORE any connection is serviced;
//! if the write fails, the listener is shut down and the error returned.
//! "A runtime that can't publish metadata is invisible to the CLI — close
//! transports rather than run undiscoverable."
//!
//! # Gaps vs the Electron host (deliberate, documented)
//! - `runtime-metadata-ownership-watch.ts` (republish-on-overwrite polling)
//!   is NOT ported. The headless host is a single runtime per userData dir;
//!   the watch exists for the Electron multi-process case. Process-level
//!   single-instance is guarded separately (`mcode_app::single_instance`).
//! - Long-poll admission fencing (`runtime_busy`), keepalive frames, and
//!   streaming dispatch are not ported: this plane answers only one-shot
//!   `status.get`.
//!
//! # Randomness note
//! `getrandom` is pinned in `[workspace.dependencies]` but is NOT a declared
//! dependency of this crate, and Cargo.toml must not change. It is also
//! unreachable transitively: `uuid` (via tauri) is feature-gated on tauri
//! `isolation` (off here) and pulls getrandom 0.4, not the pinned 0.3.4.
//! Entropy therefore comes from `std::collections::hash_map::RandomState`,
//! whose per-instance keys are derived from an OS-CSPRNG-seeded thread-local
//! and bumped per instance (see `next_random_word`). That is per-process
//! unpredictability, not a long-lived crypto stream — adequate for the
//! endpoint suffix (already pid-scoped) and the host-lifetime auth token
//! (file ACL'd 0600 exactly like the TS metadata write). If stronger
//! guarantees are ever needed, add `getrandom = "=0.3.4"` to
//! `[dependencies]` and swap `fill_random`'s body in one line.

use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncBufReadExt;

use mcode_wire::agent_exec::{ExecParams, LaneRegistry};

/// Shared exec-lane registry for `agent.execNonInteractive` /
/// `agent.cancelExec` over the CLI plane (agent-exec-handler.ts:103-113 —
/// per-(cwd, operation) lanes so cancel targets only the newest visible
/// job). One registry per process; lanes are keyed by cwd+operation.
fn agent_exec_registry() -> &'static LaneRegistry {
    static REGISTRY: OnceLock<LaneRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LaneRegistry::new)
}

use serde::Serialize;
use serde::ser::SerializeStruct;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::sync::watch;

use mcode_app::runtime_metadata::{
    RuntimeMetadata, RuntimeTransportMetadata, remove_runtime_metadata, write_runtime_metadata,
};

/// unix-socket-transport.ts:13.
const MAX_RUNTIME_RPC_CONNECTIONS: usize = 32;
/// unix-socket-transport.ts:12.
const RUNTIME_RPC_SOCKET_IDLE_TIMEOUT_MS: u64 = 30_000;
/// 16 MiB per-line cap — aligns with the relay frame `MAX_MESSAGE_SIZE`
/// budget and `NDJSON_MAX_LINE_BYTES` (src/main/daemon/ndjson.ts:1). The
/// original newline transport used 1 MiB; the shared envelope schema accepts
/// either, and 16 MiB keeps one consistent local-frame budget codebase-wide.
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
/// runtime-rpc.ts:1527 — exact original code + message for oversized frames.
const REQUEST_TOO_LARGE_MESSAGE: &str = "RPC request exceeds the maximum size";
/// protocol-version.ts:33.
const RUNTIME_PROTOCOL_VERSION: u64 = 3;
/// protocol-version.ts:34.
const MIN_COMPATIBLE_RUNTIME_CLIENT_VERSION: u64 = 2;

#[cfg(not(any(windows, unix)))]
compile_error!("runtime RPC supports windows and unix only");

// ---------------------------------------------------------------------------
// Errors — snake_case code() per the shutdown.rs LifecycleServiceError pattern.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeRpcError {
    /// Transport bind failed (primary transport).
    Bind(String),
    /// The metadata-first discipline failed: the host would be undiscoverable.
    MetadataWrite(String),
}

impl RuntimeRpcError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Bind(_) => "runtime_rpc_bind_failed",
            Self::MetadataWrite(_) => "runtime_rpc_metadata_write_failed",
        }
    }
}

impl core::fmt::Display for RuntimeRpcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bind(detail) => write!(f, "failed to bind runtime RPC transport: {detail}"),
            Self::MetadataWrite(detail) => {
                write!(f, "failed to write runtime RPC metadata: {detail}")
            }
        }
    }
}

impl std::error::Error for RuntimeRpcError {}

impl From<mcode_app::runtime_metadata::MetadataError> for RuntimeRpcError {
    fn from(error: mcode_app::runtime_metadata::MetadataError) -> Self {
        Self::MetadataWrite(error.to_string())
    }
}

// ---------------------------------------------------------------------------
// Wire frames. Keys are literal (matching runtime-rpc-envelope.ts), so the
// envelope deliberately does not use serde rename conventions.
// ---------------------------------------------------------------------------

/// Error half of the failure envelope. No `data` field — the TS failures this
/// plane ports carry none, and the CLI schema treats `data` as optional.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RpcErrorBody {
    pub code: String,
    pub message: String,
}

/// `{"id":…,"ok":false,"error":{…},"_meta":{"runtimeId":null}}` — the TS
/// `buildError` always stamps `runtimeId: null` on failure envelopes
/// (runtime-rpc.ts:1736-1738 → errors.errorResponse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcFailureFrame {
    pub id: String,
    pub error: RpcErrorBody,
}

impl Serialize for RpcFailureFrame {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut frame = serializer.serialize_struct("RpcFailure", 4)?;
        frame.serialize_field("id", &self.id)?;
        frame.serialize_field("ok", &false)?;
        frame.serialize_field("error", &self.error)?;
        frame.serialize_field("_meta", &serde_json::json!({ "runtimeId": null }))?;
        frame.end()
    }
}

/// Success envelope carrying the pre-serialized result object.
#[derive(Clone, Debug)]
pub struct RpcSuccessFrame {
    pub id: String,
    pub result: serde_json::Value,
    #[allow(dead_code)] // status contract field (runtime-rpc.ts parity)
    pub runtime_id: String,
}

impl Serialize for RpcSuccessFrame {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut frame = serializer.serialize_struct("RpcSuccess", 4)?;
        frame.serialize_field("id", &self.id)?;
        frame.serialize_field("ok", &true)?;
        frame.serialize_field("result", &self.result)?;
        frame.serialize_field(
            "_meta",
            &serde_json::json!({ "runtimeId": self.runtime_id }),
        )?;
        frame.end()
    }
}

#[derive(Clone, Debug)]
enum RpcReply {
    Success(RpcSuccessFrame),
    Failure(RpcFailureFrame),
}

impl RpcReply {
    fn to_line(&self) -> String {
        let serialized = match self {
            Self::Success(frame) => serde_json::to_string(frame),
            Self::Failure(frame) => serde_json::to_string(frame),
        };
        // Our Serialize impls are infallible in practice (struct + Value only);
        // the fallback keeps the client unstranded if that ever changes.
        serialized.unwrap_or_else(|_| {
            failure("unknown", "internal_error", "reply serialization failed").frame_string()
        })
    }

    fn frame_string(&self) -> String {
        match self {
            Self::Success(frame) => serde_json::to_string(frame),
            Self::Failure(frame) => serde_json::to_string(frame),
        }
        .unwrap_or_else(|_| {
            "{\"id\":\"unknown\",\"ok\":false,\"error\":{\"code\":\"internal_error\",\
             \"message\":\"reply serialization failed\"},\"_meta\":{\"runtimeId\":null}}"
                .to_owned()
        })
    }
}

fn failure(id: &str, code: &str, message: &str) -> RpcReply {
    RpcReply::Failure(RpcFailureFrame {
        id: id.to_owned(),
        error: RpcErrorBody {
            code: code.to_owned(),
            message: message.to_owned(),
        },
    })
}

// ---------------------------------------------------------------------------
// Request parsing + auth — exact parseAndAuth semantics (runtime-rpc.ts:1586-1608):
//   - invalid JSON            → bad_request "Invalid JSON request"   (id "unknown")
//   - missing/empty id        → bad_request "Missing request id"    (id "unknown")
//   - missing/empty method    → bad_request "Missing RPC method"    (echo id)
//   - missing/empty authToken → unauthorized "Missing auth token"
//   - mismatched authToken    → unauthorized "Invalid auth token"
// ---------------------------------------------------------------------------

struct RpcRequestLine {
    id: String,
    method: String,
}

fn parse_and_auth(line: &str, host_token: &str) -> Result<RpcRequestLine, RpcReply> {
    let parsed: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return Err(failure("unknown", "bad_request", "Invalid JSON request")),
    };
    let id = parsed.get("id").and_then(serde_json::Value::as_str);
    let Some(id) = id.filter(|id| !id.is_empty()) else {
        return Err(failure("unknown", "bad_request", "Missing request id"));
    };
    let method = parsed.get("method").and_then(serde_json::Value::as_str);
    let Some(method) = method.filter(|method| !method.is_empty()) else {
        return Err(failure(id, "bad_request", "Missing RPC method"));
    };
    let auth_token = parsed.get("authToken").and_then(serde_json::Value::as_str);
    let Some(auth_token) = auth_token.filter(|token| !token.is_empty()) else {
        return Err(failure(id, "unauthorized", "Missing auth token"));
    };
    if auth_token != host_token {
        return Err(failure(id, "unauthorized", "Invalid auth token"));
    }
    Ok(RpcRequestLine {
        id: id.to_owned(),
        method: method.to_owned(),
    })
}

// ---------------------------------------------------------------------------
// status.get result DTO — camelCase on the wire; field set matches the CLI
// projection surface (src/cli/runtime/status.ts + runtime-types.ts:67-99).
// Options skip-serialize: absent fields are the additive-compat mechanism
// old and new readers agree on.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusDto {
    pub runtime_id: String,
    #[allow(dead_code)] // status contract field (runtime-rpc.ts parity)
    #[allow(dead_code)] // status contract field (runtime-rpc.ts parity)
    pub renderer_graph_epoch: u64,
    /// "ready" once the authoritative window is up, else "unavailable".
    pub graph_status: &'static str,
    /// `authoritativeWindowId: number | null` in TS; serialized as null.
    pub authoritative_window_id: Option<i64>,
    pub live_tab_count: u64,
    pub live_leaf_count: u64,
    pub runtime_protocol_version: u64,
    pub min_compatible_runtime_client_version: u64,
    /// RuntimeCapability list; the headless host advertises none.
    pub capabilities: Vec<String>,
    pub app_version: &'static str,
    /// `NodeJS.Platform` shape: "win32" | "darwin" | "linux".
    pub host_platform: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_windows_shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floating_workspace_enabled: Option<bool>,
}

impl RuntimeStatusDto {
    fn for_host(runtime_id: &str, graph_epoch: u64, window_ready: bool) -> Self {
        Self {
            runtime_id: runtime_id.to_owned(),
            renderer_graph_epoch: graph_epoch,
            graph_status: if window_ready { "ready" } else { "unavailable" },
            authoritative_window_id: None,
            live_tab_count: 0,
            live_leaf_count: 0,
            runtime_protocol_version: RUNTIME_PROTOCOL_VERSION,
            min_compatible_runtime_client_version: MIN_COMPATIBLE_RUNTIME_CLIENT_VERSION,
            capabilities: Vec::new(),
            app_version: env!("CARGO_PKG_VERSION"),
            // NodeJS.Platform shape, not std::env::consts::OS ("windows" →
            // "win32", "macos" → "darwin"); runtime-types.ts consumes the
            // Node spelling on remote-target comparisons.
            host_platform: node_platform(),
            terminal_windows_shell: None,
            floating_workspace_enabled: None,
        }
    }
}

/// Builds the success frame for `status.get` — the pure seam under test.
fn status_reply(
    request_id: &str,
    runtime_id: &str,
    graph_epoch: u64,
    window_ready: bool,
) -> RpcReply {
    RpcReply::Success(RpcSuccessFrame {
        id: request_id.to_owned(),
        result: serde_json::to_value(RuntimeStatusDto::for_host(
            runtime_id,
            graph_epoch,
            window_ready,
        ))
        .unwrap_or(serde_json::Value::Null),
        runtime_id: runtime_id.to_owned(),
    })
}

// ---------------------------------------------------------------------------
// Reply-building seam — one request line in, one reply line out. Unit-test
// target; no Tauri runtime or socket needed. `status_snapshot` supplies
// (renderer_graph_epoch, window_ready).
// ---------------------------------------------------------------------------

fn build_reply_line(
    request_line: &str,
    host_token: &str,
    runtime_id: &str,
    status_snapshot: (u64, bool),
) -> String {
    // Oversized guard at the seam so the contract is testable without a
    // socket; the connection loop short-circuits earlier (never materializes
    // the oversized line) with the identical code + message.
    if request_line.len() > MAX_LINE_BYTES {
        return failure("unknown", "request_too_large", REQUEST_TOO_LARGE_MESSAGE).to_line();
    }
    let request = match parse_and_auth(request_line, host_token) {
        Ok(request) => request,
        Err(reply) => return reply.to_line(),
    };
    match request.method.as_str() {
        "status.get" => status_reply(
            &request.id,
            runtime_id,
            status_snapshot.0,
            status_snapshot.1,
        )
        .to_line(),
        // dispatcher.ts:51-56 — exact original code + message for unknown methods.
        _ => failure(
            &request.id,
            "method_not_found",
            &format!("Unknown method: {}", request.method),
        )
        .to_line(),
    }
}

// ---------------------------------------------------------------------------
// Transport metadata — endpoint construction stays in lockstep with
// createRuntimeTransportMetadata (runtime-rpc.ts:1791-1809). A uuid-shaped
// runtimeId sanitizes to [a-zA-Z0-9_-]{0,4}; ours is pure hex so the suffix
// is its first 4 chars, and the 'rt' fallback only fires on an all-non-word
// runtimeId (impossible for 16 fresh hex bytes, kept for lockstep).
// ---------------------------------------------------------------------------

fn endpoint_suffix(runtime_id: &str) -> String {
    let suffix: String = runtime_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(4)
        .collect();
    if suffix.is_empty() {
        "rt".to_owned()
    } else {
        suffix
    }
}

fn build_transport_metadata(user_data_path: &Path, runtime_id: &str) -> RuntimeTransportMetadata {
    let suffix = endpoint_suffix(runtime_id);
    #[cfg(windows)]
    {
        let _ = user_data_path;
        RuntimeTransportMetadata::NamedPipe {
            endpoint: format!(r"\\.\pipe\mcode-{}-{suffix}", std::process::id()),
        }
    }
    #[cfg(unix)]
    {
        RuntimeTransportMetadata::Unix {
            endpoint: user_data_path
                .join(format!("o-{}-{suffix}.sock", std::process::id()))
                .to_string_lossy()
                .into_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Entropy — see the module-level randomness note.
// ---------------------------------------------------------------------------

fn next_random_word() -> u64 {
    // Two fresh RandomStates per word; one's output folded through the
    // other's SipHash so adjacent draws diverge even on a shared seed.
    let a = std::collections::hash_map::RandomState::new();
    let b = std::collections::hash_map::RandomState::new();
    let mut hasher = b.build_hasher();
    hasher.write_u64({
        let mut inner = a.build_hasher();
        inner.write_u64(0x9E37_79B9_7F4A_7C15);
        inner.finish()
    });
    hasher.finish()
}

fn fill_random(bytes: &mut [u8]) {
    for chunk in bytes.chunks_mut(8) {
        let word = next_random_word().to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
}

/// 16 random bytes → 32 hex chars (uuid-v4-shaped runtimeId).
fn random_runtime_id() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 24 random bytes → 48 hex chars, matching the TS 24-byte token budget.
fn random_auth_token() -> String {
    let mut bytes = [0u8; 24];
    fill_random(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn started_at_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// `NodeJS.Platform` spelling of the current OS — the value the TS host
/// reports via `process.platform` ("win32" | "darwin" | "linux"), which
/// remote-target comparisons consume; `std::env::consts::OS` spells them
/// differently ("windows", "macos").
fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Listener plumbing
// ---------------------------------------------------------------------------

enum Transport {
    #[cfg(unix)]
    Unix(tokio::net::UnixListener, PathBuf),
    #[cfg(windows)]
    NamedPipe {
        endpoint: String,
        /// First instance — it is the INITIAL server of the connect chain,
        /// never parked: an unserved instance would swallow the first
        /// client's connection and hang it until timeout.
        first: tokio::net::windows::named_pipe::NamedPipeServer,
    },
}

impl Transport {
    fn bind(user_data_path: &Path, runtime_id: &str) -> Result<Self, RuntimeRpcError> {
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let endpoint = match build_transport_metadata(user_data_path, runtime_id) {
                RuntimeTransportMetadata::NamedPipe { endpoint } => endpoint,
                _ => return Err(RuntimeRpcError::Bind("named pipe metadata mismatch".into())),
            };
            // first_pipe_instance(true): fail rather than collide with a
            // stale pipe of the same name (named pipes leave no filesystem
            // entry to sweep; the pid+suffix makes collision itself rare).
            let first = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint)
                .map_err(|error| RuntimeRpcError::Bind(error.to_string()))?;
            Ok(Self::NamedPipe { endpoint, first })
        }
        #[cfg(unix)]
        {
            let path = match build_transport_metadata(user_data_path, runtime_id) {
                RuntimeTransportMetadata::Unix { endpoint } => PathBuf::from(endpoint),
                _ => {
                    return Err(RuntimeRpcError::Bind(
                        "unix socket metadata mismatch".into(),
                    ));
                }
            };
            // A stale socket from a SIGKILL'd predecessor would fail bind —
            // TS start() rmSync's the path first (unix-socket-transport.ts:55-57).
            let _ = std::fs::remove_file(&path);
            let listener = tokio::net::UnixListener::bind(&path)
                .map_err(|error| RuntimeRpcError::Bind(error.to_string()))?;
            // 0600 after bind, matching the TS chmod (unix-socket-transport.ts:72-74).
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| RuntimeRpcError::Bind(error.to_string()))?;
            Ok(Self::Unix(listener, path))
        }
    }

    #[cfg(unix)]
    fn socket_path(&self) -> Option<&Path> {
        match self {
            Self::Unix(_, path) => Some(path),
        }
    }
}

/// Bounded concurrency, mirroring the TS `server.maxConnections`. The CLI
/// holds one connection at a time; the cap is a defensive bound against a
/// runaway local client. Refusal is backpressure-by-delay, not an error
/// frame — the original's kernel backlog behaves the same way.
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);

/// Returns true when shutdown was observed while waiting for a slot.
async fn acquire_connection_slot(shutdown: &watch::Receiver<bool>) -> bool {
    loop {
        if *shutdown.borrow() {
            return true;
        }
        let current = ACTIVE_CONNECTIONS.load(Ordering::SeqCst);
        if current < MAX_RUNTIME_RPC_CONNECTIONS as u64 {
            if ACTIVE_CONNECTIONS
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return false;
            }
            continue;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn release_connection_slot() {
    let previous = ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
    debug_assert!(previous >= 1, "connection slot underflow");
}

// ---------------------------------------------------------------------------
// RuntimeRpcHost
// ---------------------------------------------------------------------------

pub struct RuntimeRpcHost {
    runtime_id: String,
    user_data_path: PathBuf,
    shutdown_tx: watch::Sender<bool>,
    #[allow(dead_code)] // status contract field (runtime-rpc.ts parity)
    renderer_graph_epoch: Arc<AtomicU64>,
    stop_called: AtomicBool,
    #[cfg(unix)]
    socket_path: Option<PathBuf>,
}

impl RuntimeRpcHost {
    /// Binds the primary transport and publishes `mcode-runtime.json` BEFORE
    /// any connection is serviced (runtime-rpc.ts:1207-1222 discipline). On
    /// metadata write failure the listener is torn down and the error
    /// returned — running undiscoverable is prohibited.
    ///
    /// The accept loop runs on `tauri::async_runtime::spawn`; [`Self::stop`]
    /// (or dropping the host) shuts it down. Idempotent.
    pub fn start(
        user_data_path: &Path,
        window_ready: Arc<AtomicBool>,
    ) -> Result<Self, RuntimeRpcError> {
        let runtime_id = random_runtime_id();
        let auth_token = random_auth_token();

        // Why: `ServerOptions::create` registers the pipe with the tokio
        // reactor, which requires a runtime context. The setup hook runs on
        // the main thread OUTSIDE any runtime — bind through the Tauri async
        // runtime so the pipe (and on POSIX the listener) gets a reactor.
        // The accept loop then runs on the same runtime (spawn below).
        let user_data = user_data_path.to_owned();
        let runtime_id_for_bind = runtime_id.clone();
        let transport = tauri::async_runtime::block_on(async move {
            Transport::bind(&user_data, &runtime_id_for_bind)
        })?;
        #[cfg(unix)]
        let socket_path = transport.socket_path().map(Path::to_path_buf);

        let transports = vec![build_transport_metadata(user_data_path, &runtime_id)];
        let metadata = RuntimeMetadata {
            runtime_id: runtime_id.clone(),
            pid: std::process::id(),
            transports,
            auth_token: Some(auth_token.clone()),
            started_at: started_at_ms(),
        };
        if let Err(error) = write_runtime_metadata(user_data_path, &metadata) {
            // Metadata-first discipline: never keep a bound-but-undiscoverable
            // host alive. Best-effort socket cleanup on POSIX; the Windows
            // pipe self-cleans when its last handle drops.
            #[cfg(unix)]
            if let Some(path) = transport.socket_path() {
                let _ = std::fs::remove_file(path);
            }
            return Err(error.into());
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let renderer_graph_epoch = Arc::new(AtomicU64::new(0));
        let serve_runtime_id = runtime_id.to_owned();
        let serve_auth_token = auth_token;
        let serve_epoch = Arc::clone(&renderer_graph_epoch);
        let mut serve_shutdown = shutdown_rx;
        tauri::async_runtime::spawn(async move {
            run_accept_loop(
                transport,
                &serve_runtime_id,
                &serve_auth_token,
                serve_epoch,
                window_ready,
                &mut serve_shutdown,
            )
            .await;
        });

        Ok(Self {
            runtime_id,
            user_data_path: user_data_path.to_owned(),
            shutdown_tx,
            renderer_graph_epoch,
            stop_called: AtomicBool::new(false),
            #[cfg(unix)]
            socket_path,
        })
    }

    /// Host runtimeId — for logs/telemetry on the wiring side.
    #[allow(dead_code)] // reserved for the CLI status contract
    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    /// Idempotent shutdown: remove the metadata file, signal the accept loop,
    /// unlink the POSIX socket file. Safe to call multiple times and from
    /// Drop. Windows named pipes need no unlink — dropping the first instance
    /// handle tears the pipe down.
    pub fn stop(&self) {
        if self.stop_called.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = remove_runtime_metadata(&self.user_data_path);
        let _ = self.shutdown_tx.send(true);
        #[cfg(unix)]
        if let Some(path) = &self.socket_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for RuntimeRpcHost {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Accept + connection loops
// ---------------------------------------------------------------------------

async fn run_accept_loop(
    transport: Transport,
    runtime_id: &str,
    auth_token: &str,
    graph_epoch: Arc<AtomicU64>,
    window_ready: Arc<AtomicBool>,
    shutdown: &mut watch::Receiver<bool>,
) {
    match transport {
        #[cfg(unix)]
        Transport::Unix(listener, _path) => {
            loop {
                if *shutdown.borrow() {
                    break;
                }
                tokio::select! {
                    // Sender dropped (host dropped) or stop() fired → exit.
                    _ = shutdown.changed() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _addr)) => {
                            if acquire_connection_slot(shutdown).await {
                                break;
                            }
                            let runtime_id = runtime_id.to_owned();
                            let auth_token = auth_token.to_owned();
                            let epoch = Arc::clone(&graph_epoch);
                            let ready = Arc::clone(&window_ready);
                            let mut conn_shutdown = shutdown.clone();
                            tauri::async_runtime::spawn(async move {
                                serve_connection(
                                    stream,
                                    &runtime_id,
                                    &auth_token,
                                    epoch,
                                    ready,
                                    &mut conn_shutdown,
                                )
                                .await;
                                release_connection_slot();
                            });
                        }
                        Err(_) => break,
                    },
                }
            }
        }
        #[cfg(windows)]
        Transport::NamedPipe { endpoint, first } => {
            use tokio::net::windows::named_pipe::ServerOptions;
            // Instance chain: the bound `first` instance IS the initial
            // server — parking it unserved would swallow the first client's
            // connection. Each connected instance is handed to a connection
            // task and immediately replaced by a fresh listener instance,
            // otherwise no new client could ever connect.
            let mut server = first;
            loop {
                if *shutdown.borrow() {
                    break;
                }
                let mut select_shutdown = shutdown.clone();
                tokio::select! {
                    _ = async {
                        let _ = select_shutdown.changed().await;
                    } => break,
                    connected = server.connect() => {
                        if connected.is_err() {
                            break;
                        }
                        let task_shutdown = shutdown.clone();
                        if acquire_connection_slot(&task_shutdown).await {
                            break;
                        }
                        let next = match ServerOptions::new().create(&endpoint) {
                            Ok(next) => next,
                            Err(_) => break,
                        };
                        let runtime_id = runtime_id.to_owned();
                        let auth_token = auth_token.to_owned();
                        let epoch = Arc::clone(&graph_epoch);
                        let ready = Arc::clone(&window_ready);
                        let mut conn_shutdown = shutdown.clone();
                        tauri::async_runtime::spawn(async move {
                            serve_connection(
                                server,
                                &runtime_id,
                                &auth_token,
                                epoch,
                                ready,
                                &mut conn_shutdown,
                            )
                            .await;
                            release_connection_slot();
                        });
                        server = next;
                    }
                }
            }
        }
    }
}

async fn serve_connection<S>(
    stream: S,
    runtime_id: &str,
    auth_token: &str,
    graph_epoch: Arc<AtomicU64>,
    window_ready: Arc<AtomicBool>,
    shutdown: &mut watch::Receiver<bool>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    loop {
        if *shutdown.borrow() {
            break;
        }
        let read = tokio::select! {
            _ = shutdown.changed() => break,
            // 30 s idle timeout — the TS socket.setTimeout destroys the
            // socket; Lines/BufReader keep any partial read buffered, so
            // data that arrived within the window is not lost on timeout
            // of a slow line.
            read = tokio::time::timeout(
                std::time::Duration::from_millis(RUNTIME_RPC_SOCKET_IDLE_TIMEOUT_MS),
                read_line_capped(&mut reader),
            ) => read,
        };
        let line = match read {
            // Idle timeout or IO error — destroy the connection.
            Err(_) | Ok(Err(_)) => break,
            // Clean peer close (EOF) — done; a partial unterminated line is
            // never dispatched, matching the newline-framed original.
            Ok(Ok(None)) => break,
            Ok(Ok(Some(ReadOutcome::TooLarge))) => {
                let reply =
                    failure("unknown", "request_too_large", REQUEST_TOO_LARGE_MESSAGE).to_line();
                if write_frame(&mut write_half, &reply).await.is_err() {
                    break;
                }
                break;
            }
            Ok(Ok(Some(ReadOutcome::Line(line)))) => line,
        };
        if line.trim().is_empty() {
            continue;
        }
        // agent-exec routes (agent-exec-handler.ts:109-113): these run on the
        // tokio runtime (spawns + lane watches), so they dispatch here — the
        // sync build_reply_line seam stays status-only for unit tests.
        if let Some(method) = parse_method_for_routing(&line, auth_token) {
            match method.as_str() {
                "agent.execNonInteractive" | "agent.cancelExec" => {
                    let reply = dispatch_agent_exec(&line, runtime_id, &method).await;
                    if write_frame(&mut write_half, &reply.to_line())
                        .await
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                _ => {}
            }
        }
        let reply_line = build_reply_line(
            &line,
            auth_token,
            runtime_id,
            (
                graph_epoch.load(Ordering::Relaxed),
                window_ready.load(Ordering::Relaxed),
            ),
        );
        if write_frame(&mut write_half, &reply_line).await.is_err() {
            break;
        }
    }
}

/// Minimal re-parse for method routing (the reply builders re-parse with
/// full auth semantics; this only classifies the method cheaply).
fn parse_method_for_routing(line: &str, auth_token: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("authToken")?.as_str()? != auth_token {
        return None;
    }
    value.get("method")?.as_str().map(str::to_owned)
}

/// agent-exec-handler.ts:85-180 — exec on a lane, cancel by lane. The
/// success frame mirrors `RpcSuccessFrame` with the ExecResult (or
/// `{ canceled }`) as the result payload.
async fn dispatch_agent_exec(request_line: &str, runtime_id: &str, method: &str) -> RpcReply {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(request_line);
    let request = match parsed {
        Ok(value) => value,
        Err(_) => return failure("unknown", "bad_request", "Invalid JSON request"),
    };
    let id = request
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let params = request
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    match method {
        "agent.execNonInteractive" => {
            let exec_params: ExecParams = match serde_json::from_value(params) {
                Ok(value) => value,
                Err(error) => {
                    return failure(
                        &id,
                        "bad_request",
                        &format!("Invalid agent.execNonInteractive params: {error}"),
                    );
                }
            };
            let result =
                mcode_wire::agent_exec::exec(agent_exec_registry(), &exec_params, None).await;
            RpcReply::Success(RpcSuccessFrame {
                id,
                result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
                runtime_id: runtime_id.to_owned(),
            })
        }
        "agent.cancelExec" => {
            let cwd = params
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let operation = params
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let canceled = mcode_wire::agent_exec::cancel(
                agent_exec_registry(),
                cwd.as_deref(),
                operation.as_deref(),
            );
            RpcReply::Success(RpcSuccessFrame {
                id,
                result: serde_json::json!({ "canceled": canceled }),
                runtime_id: runtime_id.to_owned(),
            })
        }
        _ => failure(
            &id,
            "method_not_found",
            &format!("Unknown method: {method}"),
        ),
    }
}

enum ReadOutcome {
    Line(String),
    TooLarge,
}

async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    write_half: &mut W,
    reply_line: &str,
) -> io::Result<()> {
    write_half.write_all(reply_line.as_bytes()).await?;
    write_half.write_all(b"\n").await?;
    write_half.flush().await
}

/// Reads one newline-terminated line with the byte cap applied incrementally:
/// the cap fires as soon as it is crossed, so an oversized line is never
/// fully buffered (the TS transport checks per chunk for the same reason).
async fn read_line_capped<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<ReadOutcome>> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            // EOF; a partial unterminated line is dropped.
            return Ok(None);
        }
        if let Some(pos) = available.iter().position(|&byte| byte == b'\n') {
            line.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            return Ok(Some(if line.len() > MAX_LINE_BYTES {
                ReadOutcome::TooLarge
            } else {
                ReadOutcome::Line(String::from_utf8_lossy(&line).into_owned())
            }));
        }
        line.extend_from_slice(available);
        let buffered = available.len();
        reader.consume(buffered);
        if line.len() > MAX_LINE_BYTES {
            return Ok(Some(ReadOutcome::TooLarge));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
    const RUNTIME_ID: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

    fn reply(request_line: &str, snapshot: (u64, bool)) -> Value {
        let line = build_reply_line(request_line, TOKEN, RUNTIME_ID, snapshot);
        serde_json::from_str(&line).expect("reply must be valid JSON")
    }

    fn status_request(id: &str) -> String {
        format!(r#"{{"id":"{id}","authToken":"{TOKEN}","method":"status.get","params":null}}"#)
    }

    #[tokio::test]
    async fn agent_exec_noninteractive_returns_structured_result() {
        let line = format!(
            r#"{{"id":"exec-1","authToken":"{TOKEN}","method":"agent.execNonInteractive",
                "params":{{"binary":"cmd","args":["/c","echo","hello-agent"],"operation":"test-op"}}}}"#
        );
        let reply = dispatch_agent_exec(&line, RUNTIME_ID, "agent.execNonInteractive").await;
        let json: Value = serde_json::from_str(&reply.to_line()).expect("valid JSON");
        assert_eq!(json["id"], "exec-1");
        assert_eq!(json["ok"], true);
        // ExecResult: exit_code 0, stdout contains the echoed text.
        assert_eq!(json["result"]["exitCode"], 0);
        assert!(
            json["result"]["stdout"]
                .as_str()
                .unwrap_or("")
                .contains("hello-agent")
        );
    }

    #[tokio::test]
    async fn agent_cancel_exec_reports_missing_lane() {
        let line = format!(
            r#"{{"id":"cancel-1","authToken":"{TOKEN}","method":"agent.cancelExec",
                "params":{{"cwd":"D:/definitely-not-a-lane","operation":"nope"}}}}"#
        );
        let reply = dispatch_agent_exec(&line, RUNTIME_ID, "agent.cancelExec").await;
        let json: Value = serde_json::from_str(&reply.to_line()).expect("valid JSON");
        assert_eq!(json["ok"], true);
        assert_eq!(json["result"]["canceled"], false);
    }

    #[tokio::test]
    async fn agent_exec_requires_valid_params() {
        let line = format!(
            r#"{{"id":"exec-bad","authToken":"{TOKEN}","method":"agent.execNonInteractive","params":{{}}}}"#
        );
        let reply = dispatch_agent_exec(&line, RUNTIME_ID, "agent.execNonInteractive").await;
        let json: Value = serde_json::from_str(&reply.to_line()).expect("valid JSON");
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "bad_request");
    }

    #[test]
    fn valid_status_get_shape() {
        let value = reply(&status_request("req-1"), (7, true));
        assert_eq!(value["id"], "req-1");
        assert_eq!(value["ok"], true);
        assert_eq!(value["_meta"]["runtimeId"], RUNTIME_ID);
        let result = &value["result"];
        assert_eq!(result["runtimeId"], RUNTIME_ID);
        assert_eq!(result["graphStatus"], "ready");
        assert_eq!(result["rendererGraphEpoch"], 7);
        assert_eq!(result["runtimeProtocolVersion"], 3);
        assert_eq!(result["minCompatibleRuntimeClientVersion"], 2);
        assert_eq!(result["authoritativeWindowId"], Value::Null);
        assert_eq!(result["liveTabCount"], 0);
        assert_eq!(result["liveLeafCount"], 0);
        assert_eq!(result["appVersion"], env!("CARGO_PKG_VERSION"));
        assert_eq!(result["capabilities"], serde_json::json!([]));
        assert_eq!(result["hostPlatform"], node_platform());
        // Option fields absent — the additive-compat mechanism.
        assert!(result.get("terminalWindowsShell").is_none());
        assert!(result.get("floatingWorkspaceEnabled").is_none());
    }

    #[test]
    fn graph_status_unavailable_when_window_not_ready() {
        let value = reply(&status_request("req-2"), (0, false));
        assert_eq!(value["result"]["graphStatus"], "unavailable");
    }

    #[test]
    fn wrong_token_is_unauthorized() {
        let value = reply(
            r#"{"id":"req-3","authToken":"deadbeef","method":"status.get","params":null}"#,
            (0, true),
        );
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "unauthorized");
        assert_eq!(value["error"]["message"], "Invalid auth token");
        assert_eq!(value["id"], "req-3");
        assert_eq!(value["_meta"]["runtimeId"], Value::Null);
    }

    #[test]
    fn missing_token_is_unauthorized() {
        let value = reply(
            r#"{"id":"req-4","method":"status.get","params":null}"#,
            (0, true),
        );
        assert_eq!(value["error"]["code"], "unauthorized");
        assert_eq!(value["error"]["message"], "Missing auth token");
    }

    #[test]
    fn missing_id_is_bad_request() {
        let value = reply(
            r#"{"authToken":"0123456789abcdef0123456789abcdef0123456789abcdef","method":"status.get"}"#,
            (0, true),
        );
        assert_eq!(value["error"]["code"], "bad_request");
        assert_eq!(value["error"]["message"], "Missing request id");
        assert_eq!(value["id"], "unknown");
    }

    #[test]
    fn invalid_json_is_bad_request() {
        let value = reply("not json at all", (0, true));
        assert_eq!(value["error"]["code"], "bad_request");
        assert_eq!(value["error"]["message"], "Invalid JSON request");
        assert_eq!(value["id"], "unknown");
    }

    #[test]
    fn missing_method_is_bad_request() {
        let value = reply(
            r#"{"id":"req-5","authToken":"0123456789abcdef0123456789abcdef0123456789abcdef"}"#,
            (0, true),
        );
        assert_eq!(value["error"]["code"], "bad_request");
        assert_eq!(value["error"]["message"], "Missing RPC method");
        assert_eq!(value["id"], "req-5");
    }

    #[test]
    fn unknown_method_matches_dispatcher_code() {
        let value = reply(
            r#"{"id":"req-6","authToken":"0123456789abcdef0123456789abcdef0123456789abcdef","method":"agent.exec","params":null}"#,
            (0, true),
        );
        assert_eq!(value["error"]["code"], "method_not_found");
        assert_eq!(value["error"]["message"], "Unknown method: agent.exec");
        assert_eq!(value["id"], "req-6");
    }

    #[test]
    fn oversized_line_is_request_too_large_through_the_seam() {
        let oversized = format!("{}{}", "x".repeat(MAX_LINE_BYTES), " tail");
        let line = build_reply_line(&oversized, TOKEN, RUNTIME_ID, (0, true));
        let value: Value = serde_json::from_str(&line).expect("valid JSON");
        assert_eq!(value["error"]["code"], "request_too_large");
        assert_eq!(
            value["error"]["message"],
            "RPC request exceeds the maximum size"
        );
        assert_eq!(value["id"], "unknown");
        // Exactly at the cap is still processable (bad_request on garbage
        // that fits, not request_too_large).
        let at_cap = "x".repeat(MAX_LINE_BYTES);
        let value: Value =
            serde_json::from_str(&build_reply_line(&at_cap, TOKEN, RUNTIME_ID, (0, true)))
                .expect("valid JSON");
        assert_eq!(value["error"]["code"], "bad_request");
    }

    #[test]
    fn endpoint_suffix_sanitizes_and_falls_back() {
        assert_eq!(endpoint_suffix("a1b2c3d4e5f60718293a4b5c6d7e8f90"), "a1b2");
        assert_eq!(endpoint_suffix("ab-9"), "ab-9");
        assert_eq!(endpoint_suffix("///"), "rt");
        assert_eq!(endpoint_suffix(""), "rt");
    }

    #[test]
    fn random_ids_have_expected_shape() {
        let id = random_runtime_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        let token = random_auth_token();
        assert_eq!(token.len(), 48);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(random_runtime_id(), random_runtime_id());
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mcode-runtime-rpc-test-{}-{}",
            std::process::id(),
            random_runtime_id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn round_trip_metadata(dir: &Path, runtime_id: &str) -> Value {
        let metadata = RuntimeMetadata {
            runtime_id: runtime_id.to_owned(),
            pid: std::process::id(),
            transports: vec![build_transport_metadata(dir, runtime_id)],
            auth_token: Some(random_auth_token()),
            started_at: started_at_ms(),
        };
        write_runtime_metadata(dir, &metadata).expect("metadata write");
        let raw =
            std::fs::read_to_string(dir.join("mcode-runtime.json")).expect("metadata file exists");
        serde_json::from_str(&raw).expect("valid JSON")
    }

    #[cfg(windows)]
    #[test]
    fn metadata_round_trip_named_pipe() {
        let dir = temp_dir();
        let runtime_id = random_runtime_id();
        let value = round_trip_metadata(&dir, &runtime_id);
        // camelCase on disk — the CLI reader contract.
        assert_eq!(value["runtimeId"], runtime_id);
        assert_eq!(value["pid"], std::process::id());
        assert!(value["authToken"].is_string());
        assert!(value["startedAt"].is_u64());
        // kebab-case tag for the named-pipe variant.
        assert_eq!(value["transports"][0]["kind"], "named-pipe");
        let endpoint = value["transports"][0]["endpoint"].as_str().unwrap();
        assert!(endpoint.starts_with(r"\\.\pipe\mcode-"));
        assert!(endpoint.ends_with(&endpoint_suffix(&runtime_id)));
        remove_runtime_metadata(&dir).expect("metadata remove");
        // Missing-again is Ok (idempotent remove-on-exit).
        assert!(remove_runtime_metadata(&dir).is_ok());
        assert!(!dir.join("mcode-runtime.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn metadata_round_trip_unix_socket() {
        let dir = temp_dir();
        let runtime_id = random_runtime_id();
        let value = round_trip_metadata(&dir, &runtime_id);
        assert_eq!(value["runtimeId"], runtime_id);
        assert_eq!(value["pid"], std::process::id());
        assert!(value["authToken"].is_string());
        assert!(value["startedAt"].is_u64());
        assert_eq!(value["transports"][0]["kind"], "unix");
        let endpoint = value["transports"][0]["endpoint"].as_str().unwrap();
        assert!(endpoint.starts_with("o-"));
        assert!(endpoint.ends_with(".sock"));
        remove_runtime_metadata(&dir).expect("metadata remove");
        assert!(remove_runtime_metadata(&dir).is_ok());
        assert!(!dir.join("mcode-runtime.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
