//! Renderer delivery batching: 2 ms window / 1 ms continue / ≤2 writes per
//! round / ≤16 KiB chars each, with the interactive carve-out that bypasses
//! the window for output arriving right after input.
//!
//! All numbers are production values from the original (`pty.ts:2674-2698`),
//! not arbitrary. The clock is injectable so tests use fake time.

/// Batch window: 2 ms.
pub const BATCH_WINDOW_MS: u64 = 2;
/// Continue-after window when the queue is still deep: 1 ms.
pub const BATCH_DRAIN_CONTINUE_MS: u64 = 1;
/// Max chars per flush: 16 KiB.
pub const FLUSH_CHUNK_CHARS: usize = 16 * 1024;
/// Max writes per round: 2.
pub const FLUSH_MAX_WRITES: usize = 2;
/// Interactive carve-out: output within 100 ms of last input.
pub const INTERACTIVE_WINDOW_MS: u64 = 100;
/// Interactive carve-out small-output bound: 1 KiB.
pub const INTERACTIVE_SMALL_CHARS: usize = 1024;
/// Interactive carve-out large-output bound: 16 KiB (must contain ESC[).
pub const INTERACTIVE_LARGE_CHARS: usize = 16 * 1024;
/// Interactive carve-out budget: 32 KiB.
pub const INTERACTIVE_BUDGET_CHARS: usize = 32 * 1024;

/// Injectable clock so tests use fake time instead of sleeping.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Wall-clock clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Batch state for one PTY.
pub struct OutputBatcher<C: Clock> {
    clock: C,
    /// Output sequence char count flushed in the current interactive budget.
    interactive_budget_used: usize,
    /// ms timestamp of the last input on this pty (None = never).
    last_input_ms: Option<u64>,
}

impl<C: Clock> OutputBatcher<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            interactive_budget_used: 0,
            last_input_ms: None,
        }
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// Records that input arrived (arms the interactive carve-out window).
    pub fn note_input(&mut self) {
        self.last_input_ms = Some(self.clock.now_ms());
    }

    /// The interactive carve-out: output within 100 ms of input AND
    /// (≤1 KiB OR (≤16 KiB AND contains "\x1b[")) AND within the 32 KiB
    /// budget bypasses the batcher.
    pub fn should_send_interactive_output_now(&mut self, data: &str) -> bool {
        let Some(last_input) = self.last_input_ms else {
            return false;
        };
        let now = self.clock.now_ms();
        if now.saturating_sub(last_input) > INTERACTIVE_WINDOW_MS {
            return false;
        }
        let chars = data.chars().count();
        let small_enough = chars <= INTERACTIVE_SMALL_CHARS
            || (chars <= INTERACTIVE_LARGE_CHARS && data.contains("\x1b["));
        if !small_enough {
            return false;
        }
        if self.interactive_budget_used + chars > INTERACTIVE_BUDGET_CHARS {
            return false;
        }
        self.interactive_budget_used += chars;
        true
    }

    /// Splits one coalesced pending string into flush writes: at most
    /// [`FLUSH_MAX_WRITES`] chunks of at most [`FLUSH_CHUNK_CHARS`] chars.
    /// Returns None when there is nothing to flush.
    pub fn plan_flush(&self, pending: &str) -> Option<Vec<String>> {
        if pending.is_empty() {
            return None;
        }
        let mut writes = Vec::new();
        let mut rest = pending;
        while writes.len() < FLUSH_MAX_WRITES && !rest.is_empty() {
            let take = rest
                .char_indices()
                .nth(FLUSH_CHUNK_CHARS)
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            writes.push(rest[..take].to_string());
            rest = &rest[take..];
        }
        // Anything beyond 2 writes stays pending for the next round.
        if writes.is_empty() {
            None
        } else {
            Some(writes)
        }
    }

    /// Whether the flush loop should continue another round after this one
    /// (queue still deep): continue 1 ms after a 2 ms window round.
    pub fn should_continue_draining(&self, pending_remaining: bool) -> bool {
        pending_remaining
    }

    /// The window duration in ms (2 ms per production policy).
    pub fn window_ms(&self) -> u64 {
        BATCH_WINDOW_MS
    }

    /// The continue duration in ms (1 ms per production policy).
    pub fn continue_ms(&self) -> u64 {
        BATCH_DRAIN_CONTINUE_MS
    }

    /// Resets the interactive budget window (called when the budget expires).
    pub fn reset_interactive_budget(&mut self) {
        self.interactive_budget_used = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake clock with a shared mutable counter (Arc so tests keep a handle
    /// while the batcher owns the clock).
    #[derive(Clone, Default)]
    struct FakeClock(std::sync::Arc<std::sync::atomic::AtomicU64>);

    impl Clock for FakeClock {
        fn now_ms(&self) -> u64 {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl FakeClock {
        fn advance(&self, ms: u64) {
            self.0.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn window_is_two_ms_and_continue_is_one_ms() {
        let batcher = OutputBatcher::new(FakeClock::default());
        assert_eq!(batcher.window_ms(), 2);
        assert_eq!(batcher.continue_ms(), 1);
    }

    #[test]
    fn interactive_carve_out_small_output_bypasses_window() {
        let clock = FakeClock::default();
        let mut batcher = OutputBatcher::new(clock.clone());
        batcher.note_input();
        clock.advance(INTERACTIVE_WINDOW_MS - 1);
        assert!(batcher.should_send_interactive_output_now("echo hi"));
    }

    #[test]
    fn interactive_carve_out_expires_after_100ms() {
        let clock = FakeClock::default();
        let mut batcher = OutputBatcher::new(clock.clone());
        batcher.note_input();
        clock.advance(INTERACTIVE_WINDOW_MS + 1);
        assert!(!batcher.should_send_interactive_output_now("echo hi"));
    }

    #[test]
    fn interactive_carve_out_large_tui_repaint_with_escape_bypasses() {
        let clock = FakeClock::default();
        let mut batcher = OutputBatcher::new(clock.clone());
        batcher.note_input();
        let repaint = format!("{}\x1b[2J\x1b[H", "x".repeat(INTERACTIVE_LARGE_CHARS - 8));
        assert!(batcher.should_send_interactive_output_now(&repaint));
    }

    #[test]
    fn interactive_carve_out_large_plain_output_waits() {
        let clock = FakeClock::default();
        let mut batcher = OutputBatcher::new(clock.clone());
        batcher.note_input();
        let plain = "x".repeat(INTERACTIVE_SMALL_CHARS + 1);
        assert!(!batcher.should_send_interactive_output_now(&plain));
    }

    #[test]
    fn interactive_budget_caps_bypass_at_32k() {
        let clock = FakeClock::default();
        let mut batcher = OutputBatcher::new(clock.clone());
        batcher.note_input();
        // TUI-repaint shaped chunks: ≤16 KiB AND containing ESC[.
        let chunk = format!("{}\x1b[", "x".repeat(16 * 1024 - 2));
        assert_eq!(chunk.chars().count(), 16 * 1024);
        assert!(batcher.should_send_interactive_output_now(&chunk));
        // Remaining budget: 32768 - 16384 = 16384 — a second full chunk fits.
        clock.advance(1);
        batcher.note_input();
        assert!(batcher.should_send_interactive_output_now(&chunk));
        // Budget exhausted (32768 used).
        clock.advance(1);
        batcher.note_input();
        assert!(!batcher.should_send_interactive_output_now(&chunk));
        // The budget is the binding constraint now: nothing more fits.
        assert!(!batcher.should_send_interactive_output_now("y\x1b["));
        // After reset the lane reopens.
        batcher.reset_interactive_budget();
        batcher.note_input();
        assert!(batcher.should_send_interactive_output_now(&chunk));
    }

    #[test]
    fn carve_out_never_arms_without_input() {
        let mut batcher = OutputBatcher::new(FakeClock::default());
        assert!(!batcher.should_send_interactive_output_now("hi"));
    }

    #[test]
    fn flush_plan_limits_two_writes_of_16k_chars() {
        let batcher = OutputBatcher::new(FakeClock::default());
        let pending = "x".repeat(FLUSH_CHUNK_CHARS * 2 + 5);
        let writes = batcher.plan_flush(&pending).expect("writes");
        assert_eq!(writes.len(), FLUSH_MAX_WRITES);
        assert_eq!(writes[0].chars().count(), FLUSH_CHUNK_CHARS);
        assert_eq!(writes[1].chars().count(), FLUSH_CHUNK_CHARS);
        // The 5-char tail is NOT part of this round; a follow-up flush picks
        // it up (plan is deterministic on whatever the caller still holds).
        let tail = batcher
            .plan_flush(&pending[FLUSH_CHUNK_CHARS * 2..])
            .expect("tail");
        assert_eq!(tail, vec!["xxxxx".to_string()]);
    }

    #[test]
    fn flush_plan_respects_char_boundaries_not_bytes() {
        let batcher = OutputBatcher::new(FakeClock::default());
        // Multibyte chars: 'é' is 2 bytes; the split must not slice one.
        let pending = "é".repeat(FLUSH_CHUNK_CHARS + 3);
        let writes = batcher.plan_flush(&pending).expect("writes");
        for w in &writes {
            assert!(w.chars().all(|c| c == 'é'));
        }
        // 16387 total chars: write 1 carries 16384, write 2 carries the 3-char
        // tail; both stay valid UTF-8.
        assert_eq!(
            writes.iter().map(|w| w.chars().count()).sum::<usize>(),
            FLUSH_CHUNK_CHARS + 3
        );
        assert_eq!(writes[0].chars().count(), FLUSH_CHUNK_CHARS);
        assert_eq!(writes[1].chars().count(), 3);
        assert!(writes[1].chars().all(|c| c == 'é'));
    }

    #[test]
    fn flush_plan_empty_returns_none() {
        let batcher = OutputBatcher::new(FakeClock::default());
        assert_eq!(batcher.plan_flush(""), None);
    }

    #[test]
    fn continue_draining_reflects_remaining_queue() {
        let batcher = OutputBatcher::new(FakeClock::default());
        assert!(batcher.should_continue_draining(true));
        assert!(!batcher.should_continue_draining(false));
    }
}
