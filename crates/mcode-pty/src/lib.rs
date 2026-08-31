//! Local PTY: portable-pty spawn, byte-owner streaming plane, batching,
//! shell selection. See `docs/migration/PTY_CONTRACT_SPEC.md` §5/§6 — the
//! byte owner semantics there are binding for this crate.

pub mod batcher;
pub mod byte_owner;
pub mod pty;
pub mod session;
pub mod shell;

pub use batcher::{
    BATCH_WINDOW_MS, FLUSH_CHUNK_CHARS, FLUSH_MAX_WRITES, INTERACTIVE_BUDGET_CHARS,
    INTERACTIVE_WINDOW_MS, OutputBatcher, SystemClock,
};
pub use byte_owner::{
    ByteOwner, DroppedOutput, GlobalCredit, OutputBatch, OutputChunk, RestoreMarker, RestoreReason,
};
pub use pty::{MAX_INPUT_BYTES, PtyError, PtyHandle, PtySize, PtySpec};
pub use session::PtySession;
pub use shell::{ShellLaunch, select_shell};
