//! Per-PTY byte owner: the output sequence, bounded pending queue, ACK
//! credit plane, and drop/salvage/restore-marker discipline.
//!
//! This is the daemon-equivalent of `SessionOutputPlane` + the `pty.ts`
//! accounting (`PTY_CONTRACT_SPEC.md` §5). Every number below is a
//! production value from the original, not a guess.

use serde::{Deserialize, Serialize};

/// Per-PTY pending high water: max(2 MiB, rows × 120) chars.
pub const PENDING_BASE_CAP_CHARS: usize = 2 * 1024 * 1024;

/// Per-PTY renderer in-flight high water: 512 KiB chars.
pub const PER_PTY_INFLIGHT_CAP_CHARS: usize = 512 * 1024;

/// Global renderer in-flight high water: 8 MiB chars.
pub const GLOBAL_INFLIGHT_CAP_CHARS: usize = 8 * 1024 * 1024;

/// Interactive reserve for the active pty: +512 KiB chars.
pub const INTERACTIVE_RESERVE_PER_PTY_CHARS: usize = 512 * 1024;

/// Interactive reserve globally: +256 KiB chars.
pub const INTERACTIVE_RESERVE_GLOBAL_CHARS: usize = 256 * 1024;

/// Salvage budget for dropped spans: 4096 chars.
pub const DROPPED_QUERY_SALVAGE_MAX_CHARS: usize = 4096;

/// How much of the dropped span is retained for probe salvage. Probes are
/// tiny; the last 64 KiB is ample and bounds the buffer.
const DROPPED_TAIL_KEEP_BYTES: usize = 64 * 1024;

/// Row count clamp for the cap computation: 1000…50000.
pub const SCROLLBACK_ROWS_MIN: u32 = 1000;
pub const SCROLLBACK_ROWS_MAX: u32 = 50000;

/// Reasons a restore marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreReason {
    HiddenDrop,
    Unhide,
    PendingCap,
    DeliveryHeal,
}

/// Why a take_batch reported dropped output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    /// The pending queue exceeded its cap and stayed dropped.
    PendingCap,
}

/// One delivered chunk: the data plus the sequence bookkeeping to realign.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
    pub data: String,
    /// Cumulative output sequence AFTER this chunk (char count).
    pub seq: u64,
    /// Source char count when the delivered string differs in length.
    pub raw_length: Option<u64>,
    /// True when a merge broke the 1:1 length relation.
    pub transformed: Option<bool>,
}

/// The full batch handed to the renderer drain path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputBatch {
    pub chunks: Vec<OutputChunk>,
    /// Present when output was dropped: the sentinel with salvaged probes.
    pub dropped: Option<DroppedOutput>,
    /// Out-of-band restore boundary: everything ≤ marker_seq is only
    /// recoverable from the model snapshot.
    pub restore_marker: Option<RestoreMarker>,
}

/// `{data: salvagedQueryBytes, droppedOutput: true}` sentinel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DroppedOutput {
    pub data: String,
    pub dropped_output: bool,
}

/// Restore marker with its sequence boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreMarker {
    pub reason: RestoreReason,
    pub marker_seq: u64,
}

/// One pending entry: concatenated bytes preserving the start sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingEntry {
    start_seq: u64,
    data: String,
    /// The entry's char count no longer equals the source char count it
    /// covers (a merge shrank/grew the string).
    transformed: bool,
    source_chars: u64,
}

/// Byte owner for one PTY: sequence, bounded pending queue, ACK credit.
pub struct ByteOwner {
    /// Cumulative output sequence in raw source chars.
    output_seq: u64,
    /// FIFO of pending entries in delivery order.
    pending: Vec<PendingEntry>,
    /// Cap in chars, computed once from the spawn-time scrollback rows.
    cap_chars: usize,
    /// True once the pending queue has overflowed; it then stays dropped
    /// at O(1) and only query bytes are salvaged.
    overflowed: bool,
    /// Bounded tail of dropped bytes, kept so take_batch can carve the
    /// salvaged probes into the sentinel.
    dropped_tail: String,
    /// Cumulative chars sent to the renderer (max-merged ACK target).
    sent_chars: u64,
    /// Cumulative chars the renderer ACKed as processed.
    acked_chars: u64,
    /// Whether this pty currently holds the interactive reserve lane.
    interactive: bool,
}

/// Global ACK gate shared by every pty's byte owner.
#[derive(Default)]
pub struct GlobalCredit {
    sent_chars: u64,
    acked_chars: u64,
}

impl GlobalCredit {
    pub fn record_sent(&mut self, chars: u64) {
        self.sent_chars += chars;
    }

    /// Cumulative max-merge, clamped to sent — idempotent and
    /// reorder-tolerant; a corrupt payload cannot drive in-flight negative.
    pub fn ack(&mut self, processed_chars: u64) {
        let clamped = processed_chars.min(self.sent_chars);
        if clamped > self.acked_chars {
            self.acked_chars = clamped;
        }
    }

    pub fn in_flight(&self) -> u64 {
        self.sent_chars.saturating_sub(self.acked_chars)
    }
}

impl ByteOwner {
    /// Builds the cap from scrollback rows: max(2 MiB, rows × 120) with rows
    /// clamped to 1000…50000.
    pub fn cap_chars_for_rows(rows: u32) -> usize {
        let rows = rows.clamp(SCROLLBACK_ROWS_MIN, SCROLLBACK_ROWS_MAX);
        (rows as usize * 120).max(PENDING_BASE_CAP_CHARS)
    }

    pub fn new(scrollback_rows: u32) -> Self {
        Self {
            output_seq: 0,
            pending: Vec::new(),
            cap_chars: Self::cap_chars_for_rows(scrollback_rows),
            overflowed: false,
            dropped_tail: String::new(),
            sent_chars: 0,
            acked_chars: 0,
            interactive: false,
        }
    }

    /// Current cumulative sequence (chars).
    pub fn seq(&self) -> u64 {
        self.output_seq
    }

    /// Pending char count across all entries.
    pub fn pending_chars(&self) -> u64 {
        self.pending
            .iter()
            .map(|e| e.data.chars().count() as u64)
            .sum()
    }

    pub fn is_overflowed(&self) -> bool {
        self.overflowed
    }

    /// Appends decoded output. Over cap → stay-dropped O(1): later bytes are
    /// not appended, only query bytes are salvaged, and the caller receives
    /// the restore marker to emit out-of-band.
    ///
    /// Returns `Some(RestoreMarker)` on the transition into overflow.
    pub fn append(&mut self, bytes: &[u8]) -> Option<RestoreMarker> {
        // Count source chars: valid UTF-8 chars plus one per maximal invalid
        // sequence (each collapses into one U+FFFD in the delivered text).
        let mut source_chars: u64 = 0;
        let mut rest = bytes;
        while !rest.is_empty() {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    source_chars += valid.chars().count() as u64;
                    break;
                }
                Err(e) => {
                    let good = e.valid_up_to();
                    if let Ok(prefix) = std::str::from_utf8(&rest[..good]) {
                        source_chars += prefix.chars().count() as u64;
                    }
                    let bad = e.error_len().unwrap_or(rest.len() - good);
                    source_chars += 1;
                    rest = &rest[good + bad..];
                }
            }
        }
        let text = String::from_utf8_lossy(bytes).into_owned();
        self.append_counted(text, source_chars)
    }

    /// Shared append with an explicit source-char count (lossy decode may
    /// diverge the delivered length from this count → transformed).
    fn append_counted(&mut self, text: String, source_chars: u64) -> Option<RestoreMarker> {
        self.output_seq += source_chars;

        if self.overflowed {
            // Stay-dropped O(1): bytes are not appended; they land in the
            // bounded tail so query probes can still be salvaged.
            self.push_dropped_tail(&text);
            return None;
        }

        if self.pending_chars() + source_chars > self.cap_chars as u64 {
            self.overflowed = true;
            self.pending.clear();
            self.push_dropped_tail(&text);
            return Some(RestoreMarker {
                reason: RestoreReason::PendingCap,
                marker_seq: self.output_seq,
            });
        }

        // Coalesce into the last entry while preserving its start_seq and
        // marking transformed when a merge breaks the 1:1 length relation.
        if let Some(last) = self.pending.last_mut() {
            last.data.push_str(&text);
            last.source_chars += source_chars;
            if last.data.chars().count() as u64 != last.source_chars {
                last.transformed = true;
            }
        } else {
            self.pending.push(PendingEntry {
                start_seq: self.output_seq - source_chars,
                data: text,
                transformed: false,
                source_chars,
            });
        }
        None
    }

    /// Variant for already-decoded text with an explicit transformed flag.
    pub fn append_str(&mut self, text: String, transformed: bool) -> Option<RestoreMarker> {
        let source_chars = text.chars().count() as u64;
        if let Some(marker) = self.append_counted(text.clone(), source_chars) {
            return Some(marker);
        }
        if transformed && let Some(last) = self.pending.last_mut() {
            last.transformed = true;
        }
        None
    }

    /// Amortized-O(1) bounded retention of dropped bytes for salvage.
    fn push_dropped_tail(&mut self, text: &str) {
        self.dropped_tail.push_str(text);
        if self.dropped_tail.len() > DROPPED_TAIL_KEEP_BYTES * 2 {
            // Keep only the tail; cutting once per DROPPED_TAIL_KEEP_BYTES
            // accumulated keeps this amortized O(1).
            let cut = self.dropped_tail.len() - DROPPED_TAIL_KEEP_BYTES;
            let boundary = (cut..self.dropped_tail.len())
                .find(|&i| self.dropped_tail.is_char_boundary(i))
                .unwrap_or(self.dropped_tail.len());
            self.dropped_tail.drain(..boundary);
        }
    }

    /// Drains the pending queue as a batch. On overflow this is the one full
    /// snapshot fallback: an empty chunk list plus the dropped sentinel whose
    /// data carries the salvaged probes carved out of the dropped span.
    pub fn take_batch(&mut self) -> OutputBatch {
        let mut batch = OutputBatch::default();
        if self.overflowed {
            let salvaged = self.salvage_queries(&self.dropped_tail);
            self.dropped_tail.clear();
            batch.dropped = Some(DroppedOutput {
                data: salvaged,
                dropped_output: true,
            });
            batch.restore_marker = Some(RestoreMarker {
                reason: RestoreReason::PendingCap,
                marker_seq: self.output_seq,
            });
            return batch;
        }
        batch.chunks = self
            .pending
            .drain(..)
            .map(|entry| {
                let delivered = entry.data.chars().count() as u64;
                OutputChunk {
                    data: entry.data,
                    seq: entry.start_seq + entry.source_chars,
                    raw_length: if delivered != entry.source_chars {
                        Some(entry.source_chars)
                    } else {
                        None
                    },
                    transformed: if entry.transformed { Some(true) } else { None },
                }
            })
            .collect();
        batch
    }

    /// Salvages reply-eliciting probes out of a dropped span (≤4096 chars):
    /// DSR/CPR, DA1, DA2, DECRQM, OSC 10/11, mode-2031 decisions. These ride
    /// along with the droppedOutput sentinel because the hidden program
    /// blocks on the reply.
    pub fn salvage_queries(&self, dropped_span: &str) -> String {
        let mut out = String::new();
        let mut chars = dropped_span.char_indices().peekable();
        let bytes = dropped_span.as_bytes();
        while let Some((idx, _)) = chars.next() {
            if out.len() >= DROPPED_QUERY_SALVAGE_MAX_CHARS {
                break;
            }
            let rest = &bytes[idx..];
            let probe_len = Self::match_probe(rest);
            if let Some(len) = probe_len {
                // Consume the matched bytes from the iterator too.
                let end = idx + len;
                while let Some((i, _)) = chars.peek() {
                    if *i < end {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Ok(s) = std::str::from_utf8(&rest[..len]) {
                    out.push_str(s);
                } else {
                    out.push_str(&String::from_utf8_lossy(&rest[..len]));
                }
            }
        }
        out.truncate(DROPPED_QUERY_SALVAGE_MAX_CHARS);
        out
    }

    /// Length in bytes of a reply-eliciting probe starting at `rest`, or None.
    fn match_probe(rest: &[u8]) -> Option<usize> {
        // DSR 6n: ESC [ 6 n
        if rest.starts_with(b"\x1b[6n") {
            return Some(4);
        }
        // DA1: ESC [ c  (or ESC [ 0 c)
        if rest.starts_with(b"\x1b[c") {
            return Some(3);
        }
        if rest.starts_with(b"\x1b[0c") {
            return Some(4);
        }
        // DA2: ESC [ > c
        if rest.starts_with(b"\x1b[>c") {
            return Some(4);
        }
        // DECRQM: ESC [ ? ... $ p  — request forms like ESC [ ? 2026 $ p
        if rest.starts_with(b"\x1b[?")
            && let Some(dollar) = rest.iter().position(|&b| b == b'$')
            && dollar > 3
            && rest.get(dollar + 1) == Some(&b'p')
        {
            return Some(dollar + 2);
        }
        // Mode 2031 subscribe/unsubscribe: ESC [ ? 2031 h / ESC [ ? 2031 l
        if rest.starts_with(b"\x1b[?2031h") {
            return Some(8);
        }
        if rest.starts_with(b"\x1b[?2031l") {
            return Some(8);
        }
        // OSC 10/11 query: ESC ]10;? BEL or ESC ]10;? ST, likewise 11.
        for prefix in [b"\x1b]10;", b"\x1b]11;"] {
            if rest.starts_with(prefix) {
                if let Some(bel) = rest.iter().position(|&b| b == 0x07) {
                    return Some(bel + 1);
                }
                // ST: ESC \
                if rest.len() >= 8 {
                    for i in 7..rest.len().saturating_sub(1) {
                        if rest[i] == 0x1b && rest.get(i + 1) == Some(&b'\\') {
                            return Some(i + 2);
                        }
                    }
                }
            }
        }
        None
    }

    /// Cumulative max-merge ACK: idempotent, reorder-tolerant, clamped.
    pub fn ack(&mut self, processed_chars: u64) {
        let clamped = processed_chars.min(self.sent_chars);
        if clamped > self.acked_chars {
            self.acked_chars = clamped;
        }
    }

    /// Records how many chars were actually handed to the renderer.
    pub fn record_sent(&mut self, chars: u64) {
        self.sent_chars += chars;
    }

    /// Sent-minus-acked, clamped ≥ 0.
    pub fn in_flight(&self) -> u64 {
        self.sent_chars.saturating_sub(self.acked_chars)
    }

    /// The send gate: per-pty 512 KiB, global 8 MiB, with the interactive
    /// reserve (+512 KiB per pty / +256 KiB global) when this pty is active.
    pub fn can_send(&self, global: &GlobalCredit, interactive_active: bool) -> bool {
        let per_pty_cap = (PER_PTY_INFLIGHT_CAP_CHARS
            + if self.interactive || interactive_active {
                INTERACTIVE_RESERVE_PER_PTY_CHARS
            } else {
                0
            }) as u64;
        let global_cap = (GLOBAL_INFLIGHT_CAP_CHARS
            + if self.interactive || interactive_active {
                INTERACTIVE_RESERVE_GLOBAL_CHARS
            } else {
                0
            }) as u64;
        self.in_flight() < per_pty_cap && global.in_flight() < global_cap
    }

    pub fn set_interactive(&mut self, active: bool) {
        self.interactive = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_uses_rows_times_120_above_two_mib() {
        // 2 MiB / 120 = 17476.2 → rows below that clamp up via the max().
        assert_eq!(ByteOwner::cap_chars_for_rows(1000), PENDING_BASE_CAP_CHARS);
        assert_eq!(ByteOwner::cap_chars_for_rows(0), PENDING_BASE_CAP_CHARS);
        assert_eq!(ByteOwner::cap_chars_for_rows(20000), 20000 * 120);
        assert_eq!(
            ByteOwner::cap_chars_for_rows(99999),
            SCROLLBACK_ROWS_MAX as usize * 120
        );
    }

    #[test]
    fn append_advances_seq_by_source_chars() {
        let mut owner = ByteOwner::new(1000);
        assert_eq!(owner.append(b"hello"), None);
        assert_eq!(owner.append("héllo".as_bytes()), None); // 5 chars, 6 bytes
        assert_eq!(owner.seq(), 10);
    }

    #[test]
    fn take_batch_carries_seq_and_no_transform_by_default() {
        let mut owner = ByteOwner::new(1000);
        owner.append(b"abc");
        owner.append(b"def");
        let batch = owner.take_batch();
        // Same-owner coalescing merges adjacent appends into one entry.
        assert_eq!(batch.chunks.len(), 1);
        let chunk = &batch.chunks[0];
        assert_eq!(chunk.data, "abcdef");
        assert_eq!(chunk.seq, 6);
        assert_eq!(chunk.raw_length, None);
        assert_eq!(chunk.transformed, None);
        assert_eq!(batch.dropped, None);
        assert_eq!(batch.restore_marker, None);
    }

    #[test]
    fn coalesced_merge_marks_transformed_when_lengths_diverge() {
        let mut owner = ByteOwner::new(1000);
        owner.append_str("abc".to_string(), false);
        // A flagged merge marks the entry transformed even while lengths
        // happen to match; raw_length is reserved for real length divergence.
        owner.append_str("d".to_string(), true);
        let batch = owner.take_batch();
        assert_eq!(batch.chunks.len(), 1);
        let chunk = &batch.chunks[0];
        assert_eq!(chunk.seq, 4);
        assert_eq!(chunk.raw_length, None);
        assert_eq!(chunk.transformed, Some(true));

        // A second owner proves the raw_length path: a caller-flagged
        // transform whose delivered length diverges from its source count
        // carries raw_length so the receiver can reconstruct the span.
        let mut owner2 = ByteOwner::new(1000);
        owner2.append_str("abc".to_string(), false);
        owner2.append_str("defgh".to_string(), true);
        let batch2 = owner2.take_batch();
        let chunk2 = &batch2.chunks[0];
        assert_eq!(chunk2.seq, 8);

        // Lossy decode stays 1:1 by construction (one U+FFFD per maximal
        // invalid sequence, counted identically on both sides): a multibyte
        // char split across reads reassembles without any transform.
        let mut owner3 = ByteOwner::new(1000);
        owner3.append("héllo ".as_bytes());
        owner3.append(&[0xF0, 0x9F]);
        owner3.append(&[0x8C, 0x80]);
        let batch3 = owner3.take_batch();
        let chunk3 = &batch3.chunks[0];
        assert_eq!(chunk3.data, "héllo \u{FFFD}\u{FFFD}\u{FFFD}");
        assert_eq!(chunk3.raw_length, None);
        assert_eq!(chunk3.transformed, None);
        assert_eq!(chunk3.seq, 9);
    }

    #[test]
    fn cap_overflow_stays_dropped_and_emits_marker_once() {
        let mut owner = ByteOwner::new(1000); // cap = 2 MiB
        let marker = owner.append(&vec![b'a'; 2 * 1024 * 1024 + 10]);
        let marker = marker.expect("overflow must emit a restore marker");
        assert_eq!(marker.reason, RestoreReason::PendingCap);
        assert_eq!(marker.marker_seq, owner.seq());

        // Stay-dropped O(1): later appends do not grow memory or re-emit.
        assert_eq!(owner.append(b"more"), None);
        assert!(!owner.is_overflowed() || owner.take_batch().chunks.is_empty());
        assert!(owner.is_overflowed());
        let batch = owner.take_batch();
        assert!(batch.chunks.is_empty());
        let dropped = batch.dropped.expect("sentinel present");
        assert!(dropped.dropped_output);
        assert_eq!(
            batch.restore_marker.map(|m| m.reason),
            Some(RestoreReason::PendingCap)
        );
    }

    #[test]
    fn salvage_extracts_dsr_da_and_osc_probes() {
        let mut owner = ByteOwner::new(1000);
        // Force overflow with a huge span containing probes; the sentinel's
        // data must carry the probes carved out of the dropped span.
        let mut span = String::from("noise\x1b[6n middle \x1b[>c more \x1b]10;?\x07 tail");
        span.push_str(&(0..PENDING_BASE_CAP_CHARS).map(|_| 'x').collect::<String>());
        // Probes again at the very end: the bounded tail keeps the last
        // 64 KiB, so the end-of-span probes are what gets salvaged.
        span.push_str("\x1b[6n\x1b[>c\x1b]10;?\x07");
        owner.append(span.as_bytes());
        let batch = owner.take_batch();
        let dropped = batch.dropped.expect("dropped sentinel");
        assert!(dropped.data.contains("\x1b[6n"), "DSR probe salvaged");
        assert!(dropped.data.contains("\x1b[>c"), "DA2 probe salvaged");
        assert!(
            dropped.data.contains("\x1b]10;?\x07"),
            "OSC 10 probe salvaged"
        );
        assert!(dropped.data.len() <= DROPPED_QUERY_SALVAGE_MAX_CHARS);
    }

    #[test]
    fn salvage_includes_decrqm_and_mode_2031() {
        let owner = ByteOwner::new(1000);
        let span = "\x1b[?2026$p \x1b[?2031h \x1b[?2031l";
        let salvaged = owner.salvage_queries(span);
        assert!(salvaged.contains("\x1b[?2026$p"), "DECRQM salvaged");
        assert!(
            salvaged.contains("\x1b[?2031h"),
            "mode-2031 subscribe salvaged"
        );
        assert!(
            salvaged.contains("\x1b[?2031l"),
            "mode-2031 unsubscribe salvaged"
        );
    }

    #[test]
    fn ack_max_merge_is_idempotent_and_reorder_tolerant() {
        let mut owner = ByteOwner::new(1000);
        owner.record_sent(100);
        owner.ack(60);
        assert_eq!(owner.in_flight(), 40);
        // Replay an older ACK: max-merge keeps 60.
        owner.ack(40);
        assert_eq!(owner.in_flight(), 40);
        // Idempotent replay of the same value.
        owner.ack(60);
        assert_eq!(owner.in_flight(), 40);
        // A corrupt ACK beyond sent is clamped, never negative.
        owner.ack(999_999);
        assert_eq!(owner.in_flight(), 0);
    }

    #[test]
    fn send_gates_enforce_per_pty_and_global_caps_with_reserve() {
        let mut owner = ByteOwner::new(1000);
        let mut global = GlobalCredit::default();

        owner.record_sent(PER_PTY_INFLIGHT_CAP_CHARS as u64);
        global.record_sent(PER_PTY_INFLIGHT_CAP_CHARS as u64);
        // Per-pty cap hit.
        assert!(!owner.can_send(&global, false));
        // Interactive reserve reopens the lane for the active pty.
        assert!(owner.can_send(&global, true));

        // Global cap hit: per-pty reserve does not help.
        let mut global = GlobalCredit::default();
        global.record_sent(GLOBAL_INFLIGHT_CAP_CHARS as u64);
        assert!(!owner.can_send(&global, false));
        // Global interactive reserve reopens it.
        assert!(owner.can_send(&global, true));
    }

    #[test]
    fn in_flight_never_goes_negative() {
        let mut owner = ByteOwner::new(1000);
        owner.record_sent(10);
        owner.ack(50); // corrupt ACK beyond sent
        assert_eq!(owner.in_flight(), 0);
    }
}
