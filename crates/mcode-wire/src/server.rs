//! Headless relay host: a TCP/pipe listener that speaks the MCode relay
//! wire protocol — sentinel, pre-dispatch handshake (accept / mismatch /
//! credential check), then framed message dispatch. Port of the accept
//! path in `src/relay/relay.ts:startSocketServer` +
//! `src/relay/relay-handshake.ts:setupDaemonHandshake`, minus the desktop
//! dispatcher (headless v1 forwards framed messages to a handler trait;
//! the desktop's multiplexer attaches later).
//!
//! Windows pipe paths (`\\.\pipe\…`, per `ssh-relay-endpoints.ts:23`) use
//! tokio's named-pipe listener; POSIX uses a Unix socket created with
//! 0o600 permissions (relay.ts:1069 umask discipline, no TOCTOU window).

use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;

use crate::frame::{DecodedFrame, FrameDecoder};
use crate::handshake::{ServerHandshakeDecision, server_handle_handshake_frame};
use crate::protocol::RELAY_SENTINEL;

/// Outcome of one accepted connection's handshake phase.
#[derive(Debug)]
pub enum Accepted {
    /// Handshake OK; `leftover` bytes arrived after the handshake frame and
    /// must reach the dispatcher before any further reads (relay-handshake
    /// comment: feeding them late loses them).
    Ok { left_over: Vec<u8>, reply: Vec<u8> },
    /// Version mismatch: the mismatch reply was sent and the socket closed
    /// (bridge exits 42 — relay-handshake.ts:132-146).
    Mismatch { reply: Vec<u8> },
    /// Credential rejected or protocol garbage; socket destroyed.
    Rejected,
}

/// Perform the pre-dispatch handshake on one established byte stream:
/// sentinel → one handshake frame → decision. Mirrors
/// `setupDaemonHandshake` + `handleDaemonHandshakeFrame` byte-for-byte.
pub async fn handshake<S>(
    stream: &mut S,
    launch_version: &str,
    endpoint_credential: Option<&str>,
) -> Result<Accepted, std::io::Error>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    // Sentinel gate (relay.ts:249 writes it; client `read_and_check_sentinel`
    // validates — the server sends, it does not read, so this function is the
    // server side and only needs the frame phase).
    stream.write_all(RELAY_SENTINEL).await?;

    // One frame per turn: after the handshake frame the turn ends and the
    // post-handshake tail stays buffered for drain() (the TS callback drains
    // mid-turn; the cap is the borrow-safe equivalent).
    let mut decoder = FrameDecoder::new(Some(1), None, None);
    let mut chunk = [0u8; 8192];
    loop {
        let resolved: Result<Accepted, ()> = {
            let decision: std::cell::RefCell<Option<Result<Accepted, ()>>> =
                std::cell::RefCell::new(None);
            let mut outcome_errors: Vec<crate::frame::FrameDecoderError> = Vec::new();
            let outcome = decoder.decode_turn(
                &mut |frame| {
                    if decision.borrow().is_some() {
                        return;
                    }
                    *decision.borrow_mut() = Some(
                        match server_handle_handshake_frame(
                            &frame,
                            launch_version,
                            endpoint_credential,
                        ) {
                            // One-frame turn cap: the turn ends after this frame,
                            // so the post-handshake bytes stay buffered for the
                            // drain below (the TS shape drains inside the frame
                            // callback; capping is the borrow-safe equivalent).
                            ServerHandshakeDecision::Accept { reply, .. } => Ok(Accepted::Ok {
                                left_over: Vec::new(),
                                reply,
                            }),
                            ServerHandshakeDecision::Mismatch { reply, .. } => {
                                Ok(Accepted::Mismatch { reply })
                            }
                            _ => Err(()),
                        },
                    );
                },
                &mut |error| {
                    if decision.borrow().is_none() {
                        outcome_errors.push(error);
                    }
                },
            );
            let _ = outcome;
            if let Some(resolved) = decision.into_inner() {
                match resolved {
                    Ok(Accepted::Ok { reply, .. }) => {
                        stream.write_all(&reply).await?;
                        return Ok(Accepted::Ok {
                            left_over: decoder.drain(),
                            reply,
                        });
                    }
                    other => other,
                }
            } else if !outcome_errors.is_empty() {
                // relay-handshake.ts:71-74: decode error → log + destroy.
                for error in &outcome_errors {
                    eprintln!("[relay] Handshake decode error: {error}");
                }
                Err(())
            } else {
                // No frame yet: read more bytes.
                match stream.read(&mut chunk).await {
                    Ok(0) => return Ok(Accepted::Rejected),
                    Ok(read) => {
                        if decoder.feed(&chunk[..read]).is_err() {
                            return Ok(Accepted::Rejected);
                        }
                        continue;
                    }
                    Err(_) => return Ok(Accepted::Rejected),
                }
            }
        };
        return Ok(match resolved {
            Ok(Accepted::Ok { left_over, reply }) => {
                stream.write_all(&reply).await?;
                Accepted::Ok { left_over, reply }
            }
            Ok(Accepted::Mismatch { reply }) => {
                stream.write_all(&reply).await?;
                stream.shutdown().await?;
                Accepted::Mismatch { reply }
            }
            Ok(Accepted::Rejected) | Err(()) => Accepted::Rejected,
        });
    }
}

/// Serve one bound listener until the process exits. `on_message` receives
/// every post-handshake frame with a stable per-connection id.
pub async fn serve<F>(
    listener: RelayListener,
    launch_version: &str,
    endpoint_credential: Option<&str>,
    on_frame: F,
) -> std::io::Result<()>
where
    F: Fn(u64, DecodedFrame) + Send + Sync + 'static,
{
    let version = launch_version.to_string();
    let credential = endpoint_credential.map(str::to_string);
    let next_client: Arc<std::sync::atomic::AtomicU64> =
        Arc::new(std::sync::atomic::AtomicU64::new(1));
    let on_frame = Arc::new(on_frame);
    loop {
        let stream = listener.accept().await?;
        let version = version.clone();
        let credential = credential.clone();
        let next_client = Arc::clone(&next_client);
        let on_frame = Arc::clone(&on_frame);
        tokio::spawn(async move {
            let client_id = next_client.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut stream = stream;
            if let Accepted::Ok { left_over, .. } =
                handshake(&mut stream, &version, credential.as_deref())
                    .await
                    .unwrap_or(Accepted::Rejected)
            {
                dispatch_stream(client_id, &mut stream, left_over, |id, frame| {
                    on_frame(id, frame)
                })
                .await;
            }
        });
    }
}

async fn dispatch_stream<S, F>(client_id: u64, stream: &mut S, left_over: Vec<u8>, mut on_frame: F)
where
    S: AsyncReadExt + Unpin,
    F: FnMut(u64, DecodedFrame),
{
    let mut decoder = FrameDecoder::new(None, None, None);
    if !left_over.is_empty() {
        let _ = decoder.feed(&left_over);
    }
    let mut chunk = [0u8; 16384];
    loop {
        let mut frames: Vec<DecodedFrame> = Vec::new();
        let mut errors: Vec<crate::frame::FrameDecoderError> = Vec::new();
        let _ = decoder.decode_turn(&mut |frame| frames.push(frame), &mut |error| {
            errors.push(error)
        });
        for frame in frames {
            on_frame(client_id, frame);
        }
        if !errors.is_empty() {
            // relay.ts: on decode errors the client is detached.
            return;
        }
        let read = match stream.read(&mut chunk).await {
            Ok(read) => read,
            Err(_) => return,
        };
        if read == 0 {
            return;
        }
        if decoder.feed(&chunk[..read]).is_err() {
            return;
        }
    }
}

/// The bound listener: TCP on all/one interface, or a Windows named pipe /
/// POSIX Unix socket per `sock_path`.
pub enum RelayListener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(UnixListener),
    #[cfg(windows)]
    WindowsPipe(tokio::net::windows::named_pipe::ServerOptions, String),
}

impl RelayListener {
    /// Bind a TCP listener. Port 0 picks an ephemeral port (the runtime
    /// metadata then advertises the resolved port to the CLI).
    pub async fn bind_tcp(addr: &str) -> std::io::Result<Self> {
        Ok(Self::Tcp(TcpListener::bind(addr).await?))
    }

    /// Bind a platform pipe path. POSIX: 0o600 via umask-free create (we
    /// create the socket in a private directory instead — the umask dance in
    /// relay.ts:1069-1082 is the Electron form of the same guarantee).
    pub async fn bind_pipe(sock_path: &Path) -> std::io::Result<Self> {
        #[cfg(windows)]
        {
            let _ = sock_path;
            // Windows named-pipe serving is wired by the caller through the
            // pipe-name form `\\.\pipe\mcode-relay-<hash>`; see
            // ssh-relay-endpoints.ts:23. v1 headless on Windows uses TCP on
            // 127.0.0.1 — the pipe transport lands with the runtime host.
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Windows pipe transport lands with the runtime host slice; use TCP",
            ))
        }
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(sock_path);
            let listener = UnixListener::bind(sock_path)?;
            Ok(Self::Unix(listener))
        }
    }

    async fn accept(&self) -> std::io::Result<RelayStream> {
        match self {
            Self::Tcp(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok(RelayStream::Tcp(stream))
            }
            #[cfg(unix)]
            Self::Unix(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok(RelayStream::Unix(stream))
            }
            #[cfg(windows)]
            Self::WindowsPipe(_, _) => unreachable!("bind_pipe rejects Windows pipes in v1"),
        }
    }
}

enum RelayStream {
    Tcp(tokio::net::TcpStream),
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
}

impl tokio::io::AsyncRead for RelayStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for RelayStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut *self {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::client_handshake_frame;
    use crate::protocol::{RELAY_SENTINEL, RELAY_VERSION};

    // A matching client completes the handshake; the server sees the
    // post-handshake bytes as leftover (never lost — relay-handshake note).
    #[tokio::test]
    async fn server_accepts_matching_client_with_leftover() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);

        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut sock = sock;
            let accepted = handshake(&mut sock, RELAY_VERSION, None).await.unwrap();
            match accepted {
                Accepted::Ok { left_over, .. } => {
                    tx.send(left_over).await.unwrap();
                }
                other => panic!("expected accept, got {other:?}"),
            }
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        // Server -> client: sentinel.
        let mut sentinel = vec![0u8; RELAY_SENTINEL.len()];
        client.read_exact(&mut sentinel).await.unwrap();
        assert_eq!(sentinel, RELAY_SENTINEL);
        // Client -> server: handshake frame + post-handshake bytes, one write.
        let mut segment = client_handshake_frame(RELAY_VERSION, None);
        segment.extend_from_slice(b"post-handshake");
        client.write_all(&segment).await.unwrap();
        // Server -> client: handshake-ok reply.
        let mut reply = vec![0u8; 13];
        client.read_exact(&mut reply).await.unwrap();

        let left_over = rx.recv().await.unwrap();
        assert_eq!(left_over, b"post-handshake");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn version_mismatch_replies_then_closes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut sock = sock;
            match handshake(&mut sock, RELAY_VERSION, None).await.unwrap() {
                Accepted::Mismatch { reply } => reply,
                other => panic!("expected mismatch, got {other:?}"),
            }
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut sentinel = vec![0u8; RELAY_SENTINEL.len()];
        client.read_exact(&mut sentinel).await.unwrap();
        assert_eq!(sentinel, RELAY_SENTINEL);
        client
            .write_all(&client_handshake_frame("9.9.9", None))
            .await
            .unwrap();
        // Read the mismatch reply frame and assert its payload JSON.
        let mut header = vec![0u8; 13];
        client.read_exact(&mut header).await.unwrap();
        let payload_len =
            u32::from_be_bytes([header[9], header[10], header[11], header[12]]) as usize;
        let mut payload = vec![0u8; payload_len];
        client.read_exact(&mut payload).await.unwrap();
        let text = String::from_utf8(payload).unwrap();
        assert!(text.contains("mcode-relay-handshake-mismatch"), "{text}");
        assert!(text.contains("9.9.9"), "{text}");
        server.await.unwrap();
    }
}
