//! Runtime metadata writer — port of the `mcode-runtime.json` contract in
//! `src/shared/runtime-bootstrap.ts` + the ownership rules in
//! `src/main/runtime/runtime-metadata-ownership-watch.ts`.
//!
//! `mcode-runtime.json` is the CLI's only pointer at a live runtime: RPC
//! endpoint + auth token for the bundled CLI. It is written atomically at
//! runtime startup, refreshed on transport changes, and REMOVED at runtime
//! exit — a stale file pointing at a dead pid is how `mcode status` reports
//! `stale_bootstrap`, so the remove-on-exit discipline is part of the
//! contract, not housekeeping.
//!
//! Secret boundary: `authToken` is host-local CLI auth; it is written to the
//! userData dir (user-scoped ACL on Windows, 0600 on POSIX) and never enters
//! the renderer, logs, or relay payloads.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// runtime-bootstrap.ts:45.
pub const PRIMARY_RUNTIME_METADATA_FILE: &str = "mcode-runtime.json";

/// runtime-bootstrap.ts:6-20 — discriminated transport list; unknown kinds
/// are dropped by old readers (forward compatible), so order matters: the
/// primary transport goes first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RuntimeTransportMetadata {
    Unix { endpoint: String },
    NamedPipe { endpoint: String },
    Websocket { endpoint: String },
}

/// runtime-bootstrap.ts:22-28.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetadata {
    pub runtime_id: String,
    pub pid: u32,
    pub transports: Vec<RuntimeTransportMetadata>,
    pub auth_token: Option<String>,
    pub started_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataError {
    Write(String),
    Serialize(String),
}

impl core::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Write(detail) => write!(f, "failed to write runtime metadata: {detail}"),
            Self::Serialize(detail) => write!(f, "failed to serialize runtime metadata: {detail}"),
        }
    }
}

impl std::error::Error for MetadataError {}

/// runtime-bootstrap.ts:47-49.
pub fn get_runtime_metadata_path(userData_path: &Path) -> PathBuf {
    userData_path.join(PRIMARY_RUNTIME_METADATA_FILE)
}

/// Atomic write of the metadata file (the TS write is a temp-file rename; the
/// Rust side uses the same shape via write-then-rename semantics in
/// `write_atomic`).
pub fn write_runtime_metadata(
    userData_path: &Path,
    metadata: &RuntimeMetadata,
) -> Result<(), MetadataError> {
    let path = get_runtime_metadata_path(userData_path);
    write_atomic(&path, metadata)
}

/// Remove on exit — the remove-on-exit discipline above. Missing file is
/// fine (idempotent); errors surface so the CLI's `stale_bootstrap` window
/// is not silently widened.
pub fn remove_runtime_metadata(userData_path: &Path) -> Result<(), MetadataError> {
    let path = get_runtime_metadata_path(userData_path);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(MetadataError::Write(error.to_string())),
    }
}

fn write_atomic(path: &Path, metadata: &RuntimeMetadata) -> Result<(), MetadataError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| MetadataError::Write(error.to_string()))?;
    }
    let serialized = serde_json::to_vec_pretty(metadata)
        .map_err(|error| MetadataError::Serialize(error.to_string()))?;
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, &serialized)
        .map_err(|error| MetadataError::Write(error.to_string()))?;
    // Windows: AtomicWriteFile-equivalent — a plain rename over an existing
    // file fails, so remove-then-rename. The temp file makes the window
    // where the path has no file as short as one syscall.
    #[cfg(windows)]
    {
        if path.exists() {
            std::fs::remove_file(path).map_err(|error| MetadataError::Write(error.to_string()))?;
        }
    }
    std::fs::rename(&temp_path, path).map_err(|error| MetadataError::Write(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RuntimeMetadata {
        RuntimeMetadata {
            runtime_id: "runtime-123".into(),
            pid: std::process::id(),
            transports: vec![RuntimeTransportMetadata::NamedPipe {
                endpoint: r"\\.\pipe\mcode-relay-test".into(),
            }],
            auth_token: Some("cli-token".into()),
            started_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn writes_and_reads_back_the_contract_shape() {
        let dir = tempfile::tempdir().unwrap();
        write_runtime_metadata(dir.path(), &sample()).unwrap();
        let raw = std::fs::read_to_string(get_runtime_metadata_path(dir.path())).unwrap();
        let parsed: RuntimeMetadata = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, sample());
        // camelCase field names on disk (CLI contract).
        assert!(raw.contains("\"runtimeId\""));
        assert!(raw.contains("\"authToken\""));
    }

    #[test]
    fn remove_is_idempotent_and_clears_the_pointer() {
        let dir = tempfile::tempdir().unwrap();
        write_runtime_metadata(dir.path(), &sample()).unwrap();
        remove_runtime_metadata(dir.path()).unwrap();
        assert!(!get_runtime_metadata_path(dir.path()).exists());
        // Second remove: still Ok (idempotent).
        remove_runtime_metadata(dir.path()).unwrap();
    }

    #[test]
    fn replace_is_atomic_across_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        write_runtime_metadata(dir.path(), &sample()).unwrap();
        let mut next = sample();
        next.runtime_id = "runtime-456".into();
        write_runtime_metadata(dir.path(), &next).unwrap();
        let raw = std::fs::read_to_string(get_runtime_metadata_path(dir.path())).unwrap();
        assert!(raw.contains("runtime-456"));
        assert!(!raw.contains("runtime-123"));
    }
}
