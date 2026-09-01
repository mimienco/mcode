//! Dashboard terminal-preview transport, ported from the Electron
//! `terminalPreview:*` IPC surface (original: src/main/ipc/terminal-preview.ts
//! + terminal-preview-output-stream.ts). Invariants preserved:
//!
//! - connect replays a bounded snapshot of recent output BEFORE live data
//!   (atomic boundary: the renderer never sees a torn mid-chunk tail);
//! - each emitted batch carries a monotonically increasing `seq`; the
//!   renderer ACKs the highest processed seq;
//! - ACK pressure: if the un-acked in-flight bytes exceed the window, live
//!   emission pauses (chunks keep buffering into the ring) until ACKs resume;
//! - resync: if the renderer's next-expected seq falls outside the ring (the
//!   middle was dropped), the payload is a full `snapshot` instead of a
//!   chunk so the renderer can replace, not append;
//! - unsubscribe releases the ring and stops emission deterministically.
//!
//! v1 note: mcode-pty sessions expose no subscribe API, so live output feeds
//! the ring through `observe_output` (HOOK below) which the PTY forwarder
//! must call per batch. Buffering works standalone; emission is event-driven
//! off the forwarder's existing pty://data path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

pub const TERMINAL_PREVIEW_DATA_EVENT: &str = "terminal-preview://data";

/// Ring capacity mirrors the original `INITIAL_PENDING_MAX_BYTES` +
/// `OUTPUT_PENDING_MAX_BYTES` (terminal-preview-output-stream.ts): the
/// capture ring while a snapshot serializes and the queued-pending cap are
/// both 256 KiB, so a resync (missing middle) heals from a fresh snapshot
/// instead of retaining unbounded output.
#[allow(dead_code)] // forwarder hook pending (see module doc)
const RING_CAPACITY_BYTES: usize = 256 * 1024;
/// ACK window mirrors the original `OUTPUT_IN_FLIGHT_MAX_BYTES` (512 KiB).
#[allow(dead_code)] // forwarder hook pending
const IN_FLIGHT_WINDOW_BYTES: usize = 512 * 1024;
/// One emitted batch never exceeds this many bytes — the original
/// `TERMINAL_PREVIEW_OUTPUT_BATCH_MAX_BYTES` (64 KiB); larger forwarder
/// chunks are split before push/emit with a monotonic seq per piece.
#[allow(dead_code)] // forwarder hook pending
const OUTPUT_BATCH_MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPreviewChunk {
    /// Monotonic per-connection sequence; snapshot batches share the seq of
    /// the newest chunk they contain.
    seq: u64,
    /// Incremental output; None when `snapshot` is set.
    chunk: Option<String>,
    /// Full replacement payload when the renderer's next-expected seq fell
    /// out of the ring (resync) or on cold connect.
    snapshot: Option<String>,
}

struct PreviewRing {
    /// (seq, byte-length) of retained chunks, oldest first.
    entries: Vec<(u64, usize)>,
    chunks: Vec<String>,
    bytes: usize,
    next_seq: u64,
}

impl PreviewRing {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            chunks: Vec::new(),
            bytes: 0,
            next_seq: 1,
        }
    }

    #[allow(dead_code)] // forwarder hook pending
    fn push(&mut self, chunk: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let len = chunk.len();
        self.entries.push((seq, len));
        self.chunks.push(chunk);
        self.bytes += len;
        // Why: always retain the newest chunk even when it alone exceeds the
        // ring cap — an oversized chunk must be replayable, not dropped.
        while self.bytes > RING_CAPACITY_BYTES && self.entries.len() > 1 {
            let (_, dropped) = self.entries.remove(0);
            self.chunks.remove(0);
            self.bytes -= dropped;
        }
        seq
    }

    /// Replay string for `from_seq` (exclusive); None if `from_seq` no longer
    /// has its predecessor in the ring → caller must resync.
    fn replay_from(&self, from_seq: u64) -> Option<String> {
        let start = match self.entries.first() {
            Some(&(oldest, _)) if from_seq >= oldest => from_seq,
            _ if from_seq == 0 => 0,
            _ => return None,
        };
        let mut replay = String::new();
        for (i, &(seq, _)) in self.entries.iter().enumerate() {
            if seq > start {
                replay.push_str(&self.chunks[i]);
            }
        }
        Some(replay)
    }

    #[allow(dead_code)] // forwarder hook pending
    fn oldest_seq(&self) -> Option<u64> {
        self.entries.first().map(|&(seq, _)| seq)
    }

    fn total_written(&self) -> u64 {
        self.next_seq - 1
    }
}

struct PreviewConnection {
    ring: PreviewRing,
    /// Highest seq the renderer has ACKed.
    acked_seq: u64,
    /// Un-acked emitted bytes (in-flight window accounting); buffered-only
    /// chunks (backpressure/pre-boundary) do not count — they were never sent.
    unacked_bytes: u64,
    /// Snapshot already delivered on connect (or resync).
    boundary_established: bool,
}

impl PreviewConnection {
    fn new() -> Self {
        Self {
            ring: PreviewRing::new(),
            acked_seq: 0,
            unacked_bytes: 0,
            boundary_established: false,
        }
    }

    /// True when un-acked bytes are within the window and emission may proceed.
    #[allow(dead_code)] // forwarder hook pending
    fn emission_allowed(&self) -> bool {
        self.unacked_bytes <= IN_FLIGHT_WINDOW_BYTES as u64
    }
}

#[derive(Clone, Default)]
pub struct TerminalPreviewState(Arc<Mutex<HashMap<String, PreviewConnection>>>);

impl TerminalPreviewState {
    fn connections(&self) -> std::sync::MutexGuard<'_, HashMap<String, PreviewConnection>> {
        self.0.lock().expect("preview connections lock")
    }
}

/// HOOK: the PTY forwarder (pty_commands.rs spawn_watch path) must call this
/// for every output batch destined to a previewed ptyId. Buffering is
/// standalone-safe; without the hook the ring simply stays empty.
///
/// The observe/emit surface plus its byte-cap constants are `dead_code` until
/// that forwarder commit lands; they are the reviewed, tested contract.
#[allow(dead_code)] // forwarder hook pending
pub fn observe_output(state: &TerminalPreviewState, pty_id: &str, chunk: &str) {
    if let Some(connection) = state.connections().get_mut(pty_id) {
        connection.ring.push(chunk.to_string());
    }
}

/// Atomic snapshot/live boundary: replay everything after the renderer's
/// cursor (or the full ring on a cold connect) before any live data flows.
#[tauri::command(rename_as = "terminalPreviewConnect")]
pub(crate) fn terminal_preview_connect<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, TerminalPreviewState>,
    pty_id: String,
    from_seq: Option<u64>,
) -> Result<TerminalPreviewChunk, String> {
    let mut connections = state.connections();
    let connection = connections
        .entry(pty_id.clone())
        .or_insert_with(PreviewConnection::new);
    let snapshot = match from_seq {
        Some(cursor) => connection.ring.replay_from(cursor),
        None => connection
            .ring
            .replay_from(0)
            .map(|replay| {
                if replay.is_empty() {
                    None
                } else {
                    Some(replay)
                }
            })
            .unwrap_or(None),
    };
    connection.boundary_established = true;
    if let Some(snapshot) = &snapshot {
        connection.unacked_bytes += snapshot.len() as u64;
    }
    let payload = TerminalPreviewChunk {
        seq: connection.ring.total_written(),
        chunk: None,
        snapshot,
    };
    let _ = app.emit(TERMINAL_PREVIEW_DATA_EVENT, (&pty_id, &payload));
    Ok(payload)
}

/// Renderer ACK: raises the flow window; drops the connection if unknown.
#[tauri::command(rename_as = "terminalPreviewAck")]
pub(crate) fn terminal_preview_ack(
    state: State<'_, TerminalPreviewState>,
    pty_id: String,
    seq: u64,
) -> Result<(), String> {
    let mut connections = state.connections();
    let connection = connections
        .get_mut(&pty_id.clone())
        .ok_or_else(|| "terminal preview session gone".to_string())?;
    // Why: a stale ACK (older than what we know) must never rewind the window.
    connection.acked_seq = connection.acked_seq.max(seq);
    // Why: the renderer ACKs a seq after processing everything up to it, so
    // un-acked accounting resets to whatever was emitted past that point; the
    // ring's retained bytes are the practical upper bound.
    let retained = connection.ring.bytes as u64;
    connection.unacked_bytes = connection.unacked_bytes.min(retained);
    Ok(())
}

/// Writes preview keystrokes into the owning PTY via the shared session map.
#[tauri::command(rename_as = "terminalPreviewInput")]
pub(crate) fn terminal_preview_input(
    pty_state: State<'_, crate::pty_commands::NativePtyState>,
    pty_id: String,
    data: String,
) -> Result<(), String> {
    let id: u64 = pty_id
        .parse()
        .map_err(|_| format!("invalid pty id: {pty_id}"))?;
    let session = pty_state
        .get(id)
        .ok_or_else(|| "terminal preview session gone".to_string())?;
    session
        .lock()
        .expect("pty session lock")
        .write_input(data.as_bytes())
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_as = "terminalPreviewFit")]
pub(crate) fn terminal_preview_fit(
    pty_state: State<'_, crate::pty_commands::NativePtyState>,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let id: u64 = pty_id
        .parse()
        .map_err(|_| format!("invalid pty id: {pty_id}"))?;
    let session = pty_state
        .get(id)
        .ok_or_else(|| "terminal preview session gone".to_string())?;
    // Coalesced: the reader loop applies the latest requested size.
    session.lock().expect("pty session lock").resize(cols, rows);
    Ok(())
}

#[tauri::command(rename_as = "terminalPreviewUnsubscribe")]
pub(crate) fn terminal_preview_unsubscribe(
    state: State<'_, TerminalPreviewState>,
    pty_id: String,
) -> Result<(), String> {
    state.connections().remove(&pty_id.clone());
    Ok(())
}

/// Shared emission helper for the forwarder hook: pushes + emits when the
/// window allows. Returns the emitted seq (0 = suppressed by backpressure).
///
/// Mirrors the original batching: input larger than
/// `OUTPUT_BATCH_MAX_BYTES` (64 KiB) is split on char boundaries into
/// ≤64 KiB pieces, each pushed and emitted with its own monotonic seq, so
/// one batch never exceeds the cap the renderer's ACK accounting assumes.
#[allow(dead_code)] // forwarder hook pending
pub fn emit_live_chunk<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &TerminalPreviewState,
    pty_id: &str,
    chunk: &str,
) -> u64 {
    let mut last_seq = 0;
    for piece in split_output_batch(chunk) {
        let payload = {
            let mut connections = state.connections();
            let Some(connection) = connections.get_mut(pty_id) else {
                return last_seq;
            };
            if connection.boundary_established && connection.emission_allowed() {
                connection.unacked_bytes += piece.len() as u64;
                let seq = connection.ring.push(piece.clone());
                TerminalPreviewChunk {
                    seq,
                    chunk: Some(piece.clone()),
                    snapshot: None,
                }
            } else {
                // Backpressured or pre-boundary: buffer only; the connect
                // replay or a later ACK resumes flow.
                connection.ring.push(piece.clone());
                continue;
            }
        };
        let _ = app.emit(TERMINAL_PREVIEW_DATA_EVENT, (pty_id, &payload));
        last_seq = payload.seq;
    }
    last_seq
}

/// Splits a forwarder chunk into ≤`OUTPUT_BATCH_MAX_BYTES` pieces on char
/// boundaries (original `iterateTerminalInputChunks` semantics: UTF-8 never
/// torn mid-codepoint).
#[allow(dead_code)] // forwarder hook pending
fn split_output_batch(chunk: &str) -> Vec<String> {
    if chunk.len() <= OUTPUT_BATCH_MAX_BYTES {
        return vec![chunk.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut rest = chunk;
    while rest.len() > OUTPUT_BATCH_MAX_BYTES {
        let mut end = OUTPUT_BATCH_MAX_BYTES;
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (piece, remainder) = rest.split_at(end);
        pieces.push(piece.to_owned());
        rest = remainder;
    }
    if !rest.is_empty() {
        pieces.push(rest.to_owned());
    }
    pieces
}

/// Convenience so the forwarder can resolve the preview state (registered in
/// `run()`); absent state = previews never connected. The state is managed
/// as an `Arc` so the forwarder thread can clone it without holding the
/// app-level lock.
#[allow(dead_code)] // forwarder hook pending
pub fn try_state<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<Arc<TerminalPreviewState>> {
    app.try_state::<TerminalPreviewState>()
        .map(|state| Arc::new(state.inner().clone()))
}

#[cfg(test)]
mod tests {
    use super::{OUTPUT_BATCH_MAX_BYTES, PreviewConnection, PreviewRing, split_output_batch};

    #[test]
    fn ring_replay_from_zero_is_full_history() {
        let mut ring = PreviewRing::new();
        ring.push("a".into());
        ring.push("b".into());
        assert_eq!(ring.replay_from(0).as_deref(), Some("ab"));
    }

    #[test]
    fn ring_replay_from_cursor_excludes_consumed() {
        let mut ring = PreviewRing::new();
        ring.push("a".into());
        ring.push("bc".into());
        assert_eq!(ring.replay_from(1).as_deref(), Some("bc"));
        assert_eq!(ring.replay_from(2).as_deref(), Some(""));
    }

    #[test]
    fn ring_eviction_signals_resync() {
        let mut ring = PreviewRing::new();
        ring.push("x".repeat(300_000));
        ring.push("y".repeat(300_000));
        // Oldest seq evicted; a cursor of 1 can no longer be served exactly.
        assert!(ring.oldest_seq().unwrap() > 1);
        assert_eq!(ring.replay_from(1), None);
    }

    #[test]
    fn connection_window_blocks_when_unacked() {
        let mut connection = PreviewConnection::new();
        connection.boundary_established = true;
        // 1 MiB emitted un-acked > 512 KiB in-flight window.
        for _ in 0..32 {
            connection.ring.push("z".repeat(32_768));
            connection.unacked_bytes += 32_768;
        }
        assert!(!connection.emission_allowed());
        // A full ACK resets the window (bounded by retained ring bytes).
        connection.acked_seq = connection.ring.total_written();
        connection.unacked_bytes = connection.unacked_bytes.min(connection.ring.bytes as u64);
        assert!(connection.emission_allowed());
    }

    #[test]
    fn split_output_batch_passes_small_chunks_through() {
        let pieces = split_output_batch("abc");
        assert_eq!(pieces, vec!["abc".to_owned()]);
    }

    #[test]
    fn split_output_batch_caps_pieces_at_64kib() {
        let big = "x".repeat(OUTPUT_BATCH_MAX_BYTES * 2 + 10);
        let pieces = split_output_batch(&big);
        assert!(pieces.len() >= 3);
        for piece in &pieces {
            assert!(piece.len() <= OUTPUT_BATCH_MAX_BYTES);
        }
        let total: usize = pieces.iter().map(|piece| piece.len()).sum();
        assert_eq!(total, big.len());
        // Reassembling the pieces reproduces the input exactly.
        assert_eq!(pieces.concat(), big);
    }

    #[test]
    fn split_output_batch_never_tears_utf8() {
        // 'é' is 2 bytes; a byte-cut at exactly 64 KiB would split it.
        let big = "é".repeat(OUTPUT_BATCH_MAX_BYTES); // 128 KiB
        let pieces = split_output_batch(&big);
        for piece in &pieces {
            assert!(piece.len() <= OUTPUT_BATCH_MAX_BYTES);
            assert!(std::str::from_utf8(piece.as_bytes()).is_ok());
        }
        assert_eq!(pieces.concat(), big);
    }
}
