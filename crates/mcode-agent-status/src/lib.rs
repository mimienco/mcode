//! MCode agent-status pipeline — Rust port of the Electron
//! `src/main/agent-hooks/server.ts` (3257 ln) +
//! `src/shared/agent-hook-listener.ts` (4760 ln) state machine.
//!
//! Wave 37 scope (D-013, `docs/migration/DECISIONS.md`):
//! - the loopback HTTP hook endpoint (18 provider routes + token auth +
//!   always-204 fail-open), served by `src-tauri` from this crate;
//! - provider normalization for the Claude-compatible families first
//!   (claude, openclaude, devin, kimi, pi, omp, prime-agent);
//! - the pane-keyed status store, inference endpoints, and
//!   last-status.json persistence (schema version 2, byte-compat with the
//!   Electron app's on-disk format).
//!
//! Later waves land the remaining provider normalizers (codex, gemini,
//! antigravity, amp, opencode family, cursor, droid, command-code, grok,
//! copilot, hermes) — the registry in `types.rs` names every source so
//! unsupported ones fail with a stable code instead of being silently
//! dropped.

pub mod claude_statusline;
pub mod endpoint;
pub mod http;
pub mod infer;
pub mod listener_state;
pub mod normalizers;
pub mod store;
pub mod types;
