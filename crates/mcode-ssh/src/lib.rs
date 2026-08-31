//! mcode-ssh: SSH transport for the Tauri rewrite.
//!
//! Library decision (recorded here rather than as an ADR): **russh** over
//! `ssh2`. russh is pure Rust (no libssh2 C dependency), actively maintained,
//! covers ed25519 / RSA (with rsa-sha2-256/512 promotion) / ECDSA host keys,
//! and exposes the handshake as a `check_server_key` handler we can route
//! through MCode's own host-key decision flow. `ssh2` would drag in a C
//! toolchain on every platform and offers no async API.
//!
//! Behaviors preserved from `D:/ADE/orca/src/main/ssh/`:
//! - host-key decision flow (first-use accept / changed-key reject with
//!   SHA256:unpadded-base64 fingerprints, known_hosts outranking the store)
//! - accepted-key store semantics (host+port+type scoped, validated, atomic)
//! - known_hosts read-only parsing (plaintext lines; never written by us)
//! - connection timeout (30s default), keepalive, exec with stdout/stderr/exit
//!   bounded at 16MB each, PTY channel with resize.
//!
//! No secret material (passphrases, passwords, private keys) ever appears in
//! an error message.

pub mod error;
pub mod fingerprint;
pub mod known_hosts;
pub mod session;
pub mod store;
pub mod transport;
pub mod verifier;

pub use error::SshError;
pub use session::{ExecOutput, SshPtyChannel};
pub use transport::{SshAuth, SshConnection, SshTarget};
pub use verifier::HostKeyDecision;

/// Default connect timeout, matching `CONNECT_TIMEOUT_MS` in the original.
pub const CONNECT_TIMEOUT_MS: u64 = 30_000;
/// Keepalive interval, matching `keepaliveInterval: 15_000` in the original.
pub const KEEPALIVE_INTERVAL_MS: u64 = 15_000;
/// Per-stream bound for exec output (16 MiB), matching the spec.
pub const EXEC_STREAM_MAX_BYTES: usize = 16 * 1024 * 1024;
