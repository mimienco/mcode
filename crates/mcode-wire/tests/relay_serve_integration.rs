//! In-process integration tests for the headless relay host
//! (`mcode_wire::server::serve`) — the queue item 1 evidence for the
//! relay-host wave. These drive the real TCP listener path
//! (`RelayListener::bind_tcp` → `serve`) end-to-end over the byte wire:
//! sentinel greeting, pre-dispatch handshake, and framed dispatch.
//!
//! Byte-order discipline mirrors `src/server.rs` unit tests
//! (`server.rs:333-411`): the server greets with the sentinel first, the
//! client reads it with `read_exact`, then the client sends the handshake
//! frame (optionally with post-handshake bytes in the SAME write) and reads
//! the 13-byte reply header.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use mcode_wire::frame::{DecodedFrame, encode_frame};
use mcode_wire::handshake::client_handshake_frame;
use mcode_wire::protocol::{RELAY_SENTINEL, RELAY_VERSION, message_type};
use mcode_wire::server::{RelayListener, serve};

/// Timeout guard for every blocking recv: the serve task runs forever, so a
/// missing callback delivery must fail the test instead of hanging it.
const RECV_TIMEOUT: Duration = Duration::from_secs(5);

/// A frame sent by the server-side `on_frame` callback.
type FrameEvent = (u64, DecodedFrame);

/// Spawn `serve` on a freshly bound TCP listener and return the resolved
/// address plus the receiving half of the `on_frame` event channel.
///
/// `RelayListener::bind_tcp("127.0.0.1:0")` has no address accessor (API
/// gap — see the report), so the port is reserved first with a probe
/// listener that is dropped before `bind_tcp` claims it. The serve task
/// loops on `accept` until aborted, which is the documented teardown
/// contract: the test drops its client, asserts the accepted connection's
/// dispatch completed, then aborts the accept loop.
async fn spawn_serve(
    credential: Option<&'static str>,
    capacity: usize,
) -> (
    std::net::SocketAddr,
    mpsc::Receiver<FrameEvent>,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    // Reserve an ephemeral port, release it, then bind through the real
    // public constructor.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let listener = RelayListener::bind_tcp(&addr.to_string()).await.unwrap();
    let (tx, rx) = mpsc::channel::<FrameEvent>(capacity);
    let task = tokio::spawn(serve(
        listener,
        RELAY_VERSION,
        credential,
        move |client_id, frame| {
            // `try_send` never blocks the dispatch loop; the channel capacity
            // bounds what a single test sends.
            let _ = tx.try_send((client_id, frame));
        },
    ));
    (addr, rx, task)
}

async fn connect_and_read_sentinel(addr: std::net::SocketAddr) -> tokio::net::TcpStream {
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut sentinel = vec![0u8; RELAY_SENTINEL.len()];
    client.read_exact(&mut sentinel).await.unwrap();

    client
}

/// Read one 13-byte frame header, then its payload, returning the payload.
async fn read_frame_payload(client: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut header = [0u8; 13];
    client.read_exact(&mut header).await.unwrap();
    let payload_len = u32::from_be_bytes([header[9], header[10], header[11], header[12]]) as usize;
    let mut payload = vec![0u8; payload_len];
    client.read_exact(&mut payload).await.unwrap();
    payload
}

/// 1. Happy path: full pre-dispatch handshake with matching credential, then
/// one REGULAR frame whose payload is the JSON body `{"method":"ping"}`.
/// The `on_frame` callback must receive the connection's client id and the
/// exact payload bytes; after the client drops, the serve task stays alive
/// (accept loop) and the test tears down cleanly.
#[tokio::test]
async fn full_handshake_then_framed_round_trip() {
    let (addr, mut rx, serve_task) = spawn_serve(Some("test-credential"), 16).await;

    let mut client = connect_and_read_sentinel(addr).await;

    // Handshake frame + the first REGULAR frame in ONE write — exactly the
    // byte-order the reference unit test uses for post-handshake tail bytes.
    let json_body = br#"{"method":"ping"}"#.to_vec();
    let regular = encode_json_rpc(&json_body, 7, 0);
    let mut segment = client_handshake_frame(RELAY_VERSION, Some("test-credential"));
    segment.extend_from_slice(&regular);
    client.write_all(&segment).await.unwrap();

    // Server -> client: 13-byte handshake-ok reply header (empty payload).
    let mut reply_header = [0u8; 13];
    client.read_exact(&mut reply_header).await.unwrap();
    assert_eq!(
        reply_header[0],
        message_type::HANDSHAKE,
        "reply frame must be a HANDSHAKE-type frame, header bytes {reply_header:?}"
    );
    let reply_len = u32::from_be_bytes([
        reply_header[9],
        reply_header[10],
        reply_header[11],
        reply_header[12],
    ]) as usize;
    assert!(reply_len > 0, "accept reply carries the handshake-ok body");
    let mut reply_body = vec![0u8; reply_len];
    client.read_exact(&mut reply_body).await.unwrap();
    let reply_text = String::from_utf8(reply_body).unwrap();
    assert!(
        reply_text.contains("mcode-relay-handshake-ok"),
        "accept reply must be the ok message, got {reply_text}"
    );

    // The dispatcher must forward the payload through on_frame with the
    // per-connection id.
    let (client_id, frame) = tokio::time::timeout(RECV_TIMEOUT, rx.recv())
        .await
        .expect("on_frame timed out")
        .expect("on_frame channel closed before the dispatched frame arrived");
    assert_eq!(client_id, 1, "first accepted connection gets id 1");
    assert_eq!(frame.frame_type, message_type::REGULAR);
    assert_eq!(frame.id, 7);
    assert_eq!(frame.ack, 0);
    assert_eq!(frame.payload, json_body);
    // the accepted connection's dispatch completing is proven by the event
    // above; dropping the client closes the socket and the serve task keeps
    // serving without panicking until the runtime tears it down.
    drop(client);
    // Give the accept/dispatch loop a beat; the task must still be running.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!serve_task.is_finished(), "serve task must keep serving");
    serve_task.abort();
}

/// 2. Credential mismatch: server holds `Some("test-credential")`, client
/// presents `Some("wrong")`. The server destroys the socket before any
/// dispatch: the client sees EOF and `on_frame` never fires.
#[tokio::test]
async fn wrong_credential_rejected() {
    let (addr, mut rx, serve_task) = spawn_serve(Some("test-credential"), 16).await;

    let mut client = connect_and_read_sentinel(addr).await;

    client
        .write_all(&client_handshake_frame(RELAY_VERSION, Some("wrong")))
        .await
        .unwrap();

    // The server drops the stream without a reply: the client must observe
    // EOF on the next read (either 0 bytes or a reset, both surface here).
    let mut sink = vec![0u8; 64];
    let outcome = client.read(&mut sink).await;
    match outcome {
        Ok(0) => {}
        Err(err) => panic!("expected clean EOF, got read error: {err}"),
        Ok(n) => panic!("rejected connection must send no bytes, got {n}"),
    }

    // No dispatch ever happened for this connection.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "on_frame must never fire for a rejected credential"
    );

    serve_task.abort();
}

/// 3. Version mismatch: client announces `"9.9.9"`. The server sends the
/// mismatch reply frame whose payload contains
/// `mcode-relay-handshake-mismatch`, then closes the socket; `on_frame`
/// never fires. Byte order mirrors `server.rs:377-409`.
#[tokio::test]
async fn version_mismatch_reply() {
    let (addr, mut rx, serve_task) = spawn_serve(Some("test-credential"), 16).await;

    let mut client = connect_and_read_sentinel(addr).await;

    client
        .write_all(&client_handshake_frame("9.9.9", Some("test-credential")))
        .await
        .unwrap();

    // Mismatch reply frame: read header then payload, assert the JSON body.
    let payload = read_frame_payload(&mut client).await;
    let text = String::from_utf8(payload).unwrap();
    assert!(
        text.contains("mcode-relay-handshake-mismatch"),
        "mismatch reply payload must name the error, got: {text}"
    );
    assert!(
        text.contains("9.9.9"),
        "mismatch reply must echo the version, got: {text}"
    );

    // The server shuts the write half down after the mismatch reply: the
    // socket must be closed from the client's perspective.
    let mut sink = vec![0u8; 64];
    let outcome = client.read(&mut sink).await;
    match outcome {
        Ok(0) => {}
        Err(err) => panic!("expected socket close after mismatch reply, got read error: {err}"),
        Ok(n) => panic!("expected socket close after mismatch reply, got {n} more bytes"),
    }

    // No dispatch ever happened for this connection.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "on_frame must never fire for a version mismatch"
    );

    serve_task.abort();
}

/// protocol.ts `prepareJsonRpcPayload` shape: a REGULAR frame carrying a
/// JSON-RPC body. Uses the crate's own encoder so the client bytes are
/// produced by the production code path.
fn encode_json_rpc(payload: &[u8], id: u32, ack: u32) -> Vec<u8> {
    mcode_wire::protocol::encode_json_rpc_frame(payload, id, ack).unwrap()
}

/// The unused-`encode_frame` import guard: `encode_frame` is the raw frame
/// builder the client uses for arbitrary frame types; `encode_json_rpc` is
/// the production wrapper around it. Reference both so the import list stays
/// honest (a plain regular frame for completeness).
#[test]
fn encode_frame_raw_matches_json_rpc_wrapper_layout() {
    let payload: &[u8] = br#"{"method":"ping"}"#;
    let raw = encode_frame(message_type::REGULAR, 7, 0, payload);
    let wrapped = encode_json_rpc(payload, 7, 0);
    assert_eq!(raw, wrapped);
}
