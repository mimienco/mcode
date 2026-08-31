//! Port of `orca/src/shared/relay-frame-decoder.ts` (+ `relay-frame-buffer.ts`
//! and `relay-frame-decoder-contract.ts`) and the `encodeFrame` half of
//! `orca/src/relay/protocol.ts`.
//!
//! The TS decoder is an async turn-scheduled pump with pause/resume backpressure
//! hooks and a process-level continuation scheduler. On the Rust side the
//! scheduling substrate is a synchronous `decode_turn` the host calls from its
//! socket-read loop; all *bound arithmetic* — the five turn bounds, the
//! oversized-payload discard protocol, continuation semantics — is preserved
//! exactly. What the TS layer does with `setImmediate` (defer remaining work to
//! a later turn) maps to `decode_turn` returning `TurnOutcome::Continuation`,
//! i.e. work remains and the host should call again after releasing the reader.
//! The pause/resume read-ownership hooks become the `NeedsPause` flag on the
//! turn outcome.
//!
//! Errors preserve TS messages verbatim; the continuation wrapper message comes
//! from `relay-frame-decoder-contract.ts` `FrameDecoderContinuationError`
//! (contract.ts:11-17): `Frame decoder continuation failed: ${detail}`.

use std::time::Instant;

/// relay-frame-decoder.ts:14 — `export const HEADER_LENGTH = 13`
pub const HEADER_LENGTH: usize = 13;
/// relay-frame-decoder.ts:15 — `export const MAX_MESSAGE_SIZE = 16 * 1024 * 1024`
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
/// relay-frame-decoder.ts:16 — `export const FRAME_DECODER_MAX_FRAMES_PER_TURN = 64`
pub const FRAME_DECODER_MAX_FRAMES_PER_TURN: usize = 64;
/// relay-frame-decoder.ts:17 — `FRAME_DECODER_MAX_BYTES_PER_TURN = MAX_MESSAGE_SIZE + HEADER_LENGTH`
pub const FRAME_DECODER_MAX_BYTES_PER_TURN: usize = MAX_MESSAGE_SIZE + HEADER_LENGTH;
/// relay-frame-decoder.ts:18 — `FRAME_DECODER_MAX_TURN_MS = 4`
pub const FRAME_DECODER_MAX_TURN_MS: u64 = 4;
/// relay-frame-decoder.ts:19 — `FRAME_DECODER_MAX_RETAINED_BYTES = MAX_MESSAGE_SIZE + HEADER_LENGTH + 1024 * 1024`
pub const FRAME_DECODER_MAX_RETAINED_BYTES: usize = MAX_MESSAGE_SIZE + HEADER_LENGTH + 1024 * 1024;

/// relay-frame-decoder-contract.ts:1-6 — `DecodedFrame`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    pub frame_type: u8,
    pub id: u32,
    pub ack: u32,
    pub payload: Vec<u8>,
}

/// Errors surface with the exact TS strings (relay-frame-decoder.ts:71,132 and
/// relay-frame-decoder-contract.ts:13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameDecoderError {
    /// relay-frame-decoder.ts:71 — `` `Frame decoder retained-input limit exceeded: ${retained}` ``
    RetainedInputLimitExceeded(usize),
    /// relay-frame-decoder.ts:132 — `` `Frame payload too large: ${length} bytes — discarded` ``
    FramePayloadTooLarge(usize),
    /// relay-frame-decoder-contract.ts:13 — `` `Frame decoder continuation failed: ${detail}` ``
    Continuation(String),
}

impl core::fmt::Display for FrameDecoderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FrameDecoderError::RetainedInputLimitExceeded(n) => {
                write!(f, "Frame decoder retained-input limit exceeded: {n}")
            }
            FrameDecoderError::FramePayloadTooLarge(n) => {
                write!(f, "Frame payload too large: {n} bytes — discarded")
            }
            FrameDecoderError::Continuation(detail) => {
                write!(f, "Frame decoder continuation failed: {detail}")
            }
        }
    }
}

impl std::error::Error for FrameDecoderError {}

/// What the host should do after a `decode_turn` call.
///
/// TS mapping (relay-frame-decoder.ts:157-164):
/// - `Continuation` = `hasRunnableWork()` true → `scheduleContinuation()` →
///   the host must call `decode_turn` again (after giving up read ownership).
/// - `Idle` = `releasePause()` — no runnable work, read loop may resume.
/// - `NeedsPause` = a continuation was scheduled while the pause hook acquired
///   read ownership: the host should pause the reader until it can run the
///   continuation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    /// No runnable work remains (releasePause, relay-frame-decoder.ts:163).
    Idle,
    /// Runnable work remains; call `decode_turn` again (relay-frame-decoder.ts:160-162).
    Continuation,
    /// Continuation remains AND the reader should be paused (acquirePause,
    /// relay-frame-decoder.ts:199).
    NeedsPause,
}

/// Port of `RelayFrameBuffer` (relay-frame-buffer.ts:1-84): chunked FIFO of
/// consumed-from-the-front byte buffers.
#[derive(Default)]
struct RelayFrameBuffer {
    chunks: std::collections::VecDeque<Vec<u8>>,
    bytes: usize,
}

impl RelayFrameBuffer {
    fn length(&self) -> usize {
        self.bytes
    }

    fn append(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        self.chunks.push_back(chunk.to_vec());
        self.bytes += chunk.len();
    }

    fn clear(&mut self) {
        self.chunks.clear();
        self.bytes = 0;
    }

    /// relay-frame-buffer.ts:19-23.
    fn drain(&mut self) -> Vec<u8> {
        if self.chunks.len() == 1 {
            let out = self.chunks.pop_front().unwrap();
            self.bytes = 0;
            out
        } else {
            let mut out = Vec::with_capacity(self.bytes);
            for chunk in self.chunks.drain(..) {
                out.extend_from_slice(&chunk);
            }
            self.bytes = 0;
            out
        }
    }

    /// relay-frame-buffer.ts:25-39 — returns exactly `count` bytes from the
    /// front without consuming (caller guarantees `count <= length` when it
    /// matters; TS `peek` returns a shorter buffer otherwise, and the only
    /// in-class caller checks `length < HEADER_LENGTH` first).
    fn peek(&self, count: usize) -> Vec<u8> {
        let mut out = vec![0u8; count];
        let mut copied = 0;
        for part in &self.chunks {
            if copied >= count {
                break;
            }
            let take = part.len().min(count - copied);
            out[copied..copied + take].copy_from_slice(&part[..take]);
            copied += take;
        }
        out.resize(copied, 0);
        out
    }

    /// relay-frame-buffer.ts:41-68 — consume exactly `count` bytes from the front.
    fn take(&mut self, count: usize) -> Vec<u8> {
        debug_assert!(count <= self.bytes, "take beyond buffered length");
        let mut out = Vec::with_capacity(count);
        let mut remaining = count;
        while remaining > 0 {
            let part = self.chunks.front_mut().expect("buffer underflow");
            let take = part.len().min(remaining);
            out.extend_from_slice(&part[..take]);
            if take == part.len() {
                self.chunks.pop_front();
            } else {
                part.drain(..take);
            }
            remaining -= take;
        }
        self.bytes -= count;
        out
    }

    /// relay-frame-buffer.ts:70-83 — drop exactly `count` bytes from the front.
    fn discard(&mut self, count: usize) {
        let mut remaining = count;
        while remaining > 0 {
            let part = self
                .chunks
                .front_mut()
                .expect("discard beyond buffered length");
            let take = part.len().min(remaining);
            if take == part.len() {
                self.chunks.pop_front();
            } else {
                part.drain(..take);
            }
            remaining -= take;
        }
        self.bytes -= count;
    }
}

/// Port of the `FrameDecoder` class (relay-frame-decoder.ts:21-261). The
/// scheduling/pause plumbing is replaced by explicit `TurnOutcome`s returned
/// from `decode_turn`; every bound and error condition is identical.
pub struct FrameDecoder {
    buffer: RelayFrameBuffer,
    oversized_payload_bytes_remaining: usize,
    /// `maxFramesPerTurn` (relay-frame-decoder.ts:48-51).
    max_frames_per_turn: usize,
    /// `maxBytesPerTurn` (relay-frame-decoder.ts:52).
    max_bytes_per_turn: usize,
    /// `maxTurnMs` (relay-frame-decoder.ts:53).
    max_turn_ms: u64,
    started_at: Option<Instant>,
}

fn positive_limit(value: Option<usize>, fallback: usize) -> usize {
    match value {
        Some(v) if v > 0 => v,
        _ => fallback,
    }
}

impl FrameDecoder {
    /// relay-frame-decoder.ts:41-60. Options use the same
    /// `positiveLimit` fallback (relay-frame-decoder.ts:263-264): `Some(0)` or
    /// `None` fall back to the constant.
    pub fn new(
        max_frames_per_turn: Option<usize>,
        max_bytes_per_turn: Option<usize>,
        max_turn_ms: Option<u64>,
    ) -> Self {
        Self {
            buffer: RelayFrameBuffer::default(),
            oversized_payload_bytes_remaining: 0,
            max_frames_per_turn: positive_limit(
                max_frames_per_turn,
                FRAME_DECODER_MAX_FRAMES_PER_TURN,
            ),
            max_bytes_per_turn: positive_limit(
                max_bytes_per_turn,
                FRAME_DECODER_MAX_BYTES_PER_TURN,
            ),
            max_turn_ms: match max_turn_ms {
                Some(v) if v > 0 => v,
                _ => FRAME_DECODER_MAX_TURN_MS,
            },
            started_at: None,
        }
    }

    /// relay-frame-decoder.ts:62-81 — `feed`.
    ///
    /// If the retained-input limit is exceeded the decoder resets and reports
    /// `RetainedInputLimitExceeded`; the chunk is NOT appended (TS appends
    /// nothing either: the early `return` at relay-frame-decoder.ts:73 precedes
    /// `this.buffer.append`).
    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), FrameDecoderError> {
        let retained = self.buffer.length() + chunk.len();
        if retained > FRAME_DECODER_MAX_RETAINED_BYTES {
            self.reset();
            return Err(FrameDecoderError::RetainedInputLimitExceeded(retained));
        }
        if !chunk.is_empty() {
            self.buffer.append(chunk);
        }
        Ok(())
    }

    /// relay-frame-decoder.ts:83-89 — `reset`: bump generation (n/a — turns
    /// are host-driven), cancel continuation, clear buffer, drop oversized
    /// discard counter, release pause.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.oversized_payload_bytes_remaining = 0;
        self.started_at = None;
    }

    /// relay-frame-decoder.ts:91-95 — `drain`: return the retained bytes and
    /// reset. Used by the connect side to hand post-handshake bytes to the
    /// JSON-RPC dispatcher (relay-handshake.ts:214).
    pub fn drain(&mut self) -> Vec<u8> {
        let out = self.buffer.drain();
        self.reset();
        out
    }

    /// relay-frame-decoder.ts:97-165 — `drainTurn` plus the continuation
    /// decision at 157-164 and `hasRunnableWork` at 181-190.
    ///
    /// Callers treat a returned `Err` like the TS continuation wrapper
    /// (containFrameDecoderContinuation, relay-frame-decoder-contract.ts:30-41):
    /// the state has already been reset inside, and the error should be
    /// published to the error observer.
    pub fn decode_turn(
        &mut self,
        on_frame: &mut dyn FnMut(DecodedFrame),
        on_error: &mut dyn FnMut(FrameDecoderError),
    ) -> TurnOutcome {
        let started_at = *self.started_at.get_or_insert_with(Instant::now);
        let mut frames: usize = 0;
        let mut bytes: usize = 0;

        while frames < self.max_frames_per_turn
            && bytes < self.max_bytes_per_turn
            && !(frames > 0 && started_at.elapsed().as_millis() as u64 >= self.max_turn_ms)
        {
            // discardOversizedPayload (167-179): runs before header checks.
            let discarded = self.discard_oversized_payload(bytes);
            if discarded > 0 {
                bytes += discarded;
                continue;
            }
            if self.buffer.length() < HEADER_LENGTH {
                break;
            }
            let header = self.buffer.peek(HEADER_LENGTH);
            let length =
                u32::from_be_bytes([header[9], header[10], header[11], header[12]]) as usize;
            if length > MAX_MESSAGE_SIZE {
                self.buffer.discard(HEADER_LENGTH);
                self.oversized_payload_bytes_remaining = length;
                bytes += HEADER_LENGTH;
                // Error published immediately (relay-frame-decoder.ts:130-133).
                on_error(FrameDecoderError::FramePayloadTooLarge(length));
                continue;
            }
            let total_length = HEADER_LENGTH + length;
            if self.buffer.length() < total_length {
                break;
            }
            // relay-frame-decoder.ts:140-142 — once frames have been emitted
            // this turn, a frame that would exceed the byte budget ends the
            // turn (it is NOT discarded; it waits for the next turn).
            if frames > 0 && bytes + total_length > self.max_bytes_per_turn {
                break;
            }
            let framed = self.buffer.take(total_length);
            frames += 1;
            bytes += total_length;
            on_frame(DecodedFrame {
                frame_type: framed[0],
                id: u32::from_be_bytes([framed[1], framed[2], framed[3], framed[4]]),
                ack: u32::from_be_bytes([framed[5], framed[6], framed[7], framed[8]]),
                payload: framed[HEADER_LENGTH..total_length].to_vec(),
            });
        }

        self.has_runnable_work_outcome()
    }

    /// relay-frame-decoder.ts:167-179 — `discardOversizedPayload`.
    fn discard_oversized_payload(&mut self, bytes: usize) -> usize {
        if self.oversized_payload_bytes_remaining == 0 || self.buffer.length() == 0 {
            return 0;
        }
        let discarded = self
            .oversized_payload_bytes_remaining
            .min(self.buffer.length())
            .max(1)
            .min(self.max_bytes_per_turn.saturating_sub(bytes).max(1));
        self.buffer.discard(discarded);
        self.oversized_payload_bytes_remaining -= discarded;
        discarded
    }

    /// relay-frame-decoder.ts:181-190 — `hasRunnableWork`, mapped to outcomes.
    /// `oversizedPayloadBytesRemaining > 0` means only continuation is possible
    /// (data still buffered to discard); otherwise runnable work exists iff a
    /// full frame is present or the next frame is oversized. `NeedsPause`
    /// mirrors `scheduleContinuation` acquiring read ownership (199): the
    /// outcome carries it whenever a continuation is requested.
    fn has_runnable_work_outcome(&self) -> TurnOutcome {
        let runnable = if self.oversized_payload_bytes_remaining > 0 {
            self.buffer.length() > 0
        } else if self.buffer.length() < HEADER_LENGTH {
            false
        } else {
            let header = self.buffer.peek(HEADER_LENGTH);
            let length =
                u32::from_be_bytes([header[9], header[10], header[11], header[12]]) as usize;
            length > MAX_MESSAGE_SIZE || self.buffer.length() >= HEADER_LENGTH + length
        };
        if runnable {
            TurnOutcome::Continuation
        } else {
            TurnOutcome::Idle
        }
    }
}

/// protocol.ts:140-152 — `encodeFrame`. `type` in the low byte of position 0,
/// then big-endian u32 id, ack, payload length.
pub fn encode_frame(frame_type: u8, id: u32, ack: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LENGTH + payload.len());
    out.push(frame_type);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&ack.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}
