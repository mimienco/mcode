//! Local log-tail watcher commands (spec `docs/migration/FS_CONTRACT_SPEC.md`
//! §4 rows `fs:readLocalLogTail`, `fs:startLocalLogTail`,
//! `fs:stopLocalLogTail`, event `fs:localLogTailChanged`).
//!
//! Original semantics: `orca/src/main/ipc/local-log-tail.ts:59-96` (watcher
//! registry + event push), `orca/src/main/ai-vault/local-log-tail-reader.ts`
//! (256 KB ranged read + identity construction), and
//! `orca/src/shared/local-log-tail-types.ts:1-26` (wire shapes). Split from
//! `filesystem_commands.rs` because the watcher registry is stateful and
//! emits its own event stream.

// Why: this in-flight slice has types not yet consumed by every command; clippy sweep when wiring completes (in-flight log-tail slice: emitter/payload types wired by the start/stop commands).
#![allow(dead_code)]
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use crossbeam_channel::Receiver;
use notify::Watcher;
use tauri::{AppHandle, Emitter};

use crate::DesktopCommandError;

/// Renderer-facing log-tail event name (spec §4 `fs:localLogTailChanged`).
pub(crate) const FS_LOG_TAIL_CHANGED_EVENT: &str = "fs://log-tail-changed";

/// Why: 256 KB chunks keep each IPC payload small while still draining a
/// growing log in few round trips (`local-log-tail-types.ts:1`,
/// `local-log-tail-reader.ts:47`).
pub(crate) const LOCAL_LOG_TAIL_CHUNK_BYTES: u64 = 256 * 1024;

/// Read result (`local-log-tail-types.ts:9-16`).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogTailReadResult {
    pub content_base64: String,
    pub next_byte_offset: u64,
    pub file_size: u64,
    pub file_identity: String,
    pub has_more: bool,
    pub reset: bool,
}

/// Watch push payload (`local-log-tail-types.ts:23-26`).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogTailChangedPayload {
    pub subscription_id: String,
    pub event_type: &'static str,
}

/// Pushes `fs://log-tail-changed` (the original sends per `WebContents`,
/// `local-log-tail.ts:75-81`; Tauri watchers are broadcast events per
/// spec §1).
#[derive(Clone)]
pub(crate) struct LogTailEmitter {
    app: AppHandle,
    subscription_id: String,
}

impl LogTailEmitter {
    fn send(&self, event_type: &'static str) {
        let payload = LogTailChangedPayload {
            subscription_id: self.subscription_id.clone(),
            event_type,
        };
        if let Err(error) = self.app.emit(FS_LOG_TAIL_CHANGED_EVENT, &payload) {
            eprintln!("[fs] failed to emit log-tail event: {error}");
        }
    }
}

impl LogTailEmitter {
    pub(crate) fn new(app: AppHandle, subscription_id: String) -> Self {
        Self {
            app,
            subscription_id,
        }
    }
}

/// Mirrors `localLogFileIdentity`
/// (`orca/src/main/ai-vault/local-log-tail-reader.ts:7-15`): inode catches
/// atomic replacement on POSIX while birthtime covers platforms/filesystems
/// that report an unusable inode value.
fn local_log_file_identity(metadata: &std::fs::Metadata) -> String {
    let birthtime_ms = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis() as u64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}:{}", metadata.dev(), metadata.ino(), birthtime_ms)
    }
    #[cfg(not(unix))]
    {
        // Why: the stable Windows metadata API does not expose the file
        // index (`windows_by_handle` is unstable), so identity falls back to
        // creation time — which still distinguishes atomic replacement
        // (reader.ts:12-13: "birthtime covers platforms/filesystems that
        // report an unusable inode value").
        format!("0:0:{birthtime_ms}")
    }
}

/// Port of `readLocalLogTailRange`
/// (`orca/src/main/ai-vault/local-log-tail-reader.ts:17-78`). Wire shape per
/// `local-log-tail-types.ts:9-16`; 256 KB chunks; identity mismatch or an
/// offset past EOF ⇒ `reset: true`.
pub(crate) fn read_local_log_tail_range(
    path: &Path,
    from_byte_offset: u64,
    expected_identity: Option<&str>,
) -> Result<LogTailReadResult, DesktopCommandError> {
    let operation = "fs_read_local_log_tail";
    let mut file = File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            command_error(
                "fs_not_found",
                operation,
                "Local log tail target is missing",
            )
        } else {
            command_error(
                "fs_permission_denied",
                operation,
                format!("failed to open log: {error}"),
            )
        }
    })?;
    let initial_metadata = file.metadata().map_err(|error| {
        command_error(
            "fs_permission_denied",
            operation,
            format!("failed to stat log: {error}"),
        )
    })?;
    if !initial_metadata.is_file() {
        return Err(command_error(
            "fs_invalid_argument",
            operation,
            "Local log tail target is not a file",
        ));
    }
    let file_size = initial_metadata.len();
    let file_identity = local_log_file_identity(&initial_metadata);

    // Why: offset beyond EOF or a stale identity means the renderer's
    // baseline is invalid — reset instead of reading garbage
    // (`local-log-tail-reader.ts:33-45`).
    if from_byte_offset > file_size
        || expected_identity.is_some_and(|expected| expected != file_identity)
    {
        return Ok(LogTailReadResult {
            content_base64: String::new(),
            next_byte_offset: 0,
            file_size,
            file_identity,
            has_more: file_size > 0,
            reset: true,
        });
    }

    let bytes_to_read = LOCAL_LOG_TAIL_CHUNK_BYTES.min(file_size - from_byte_offset);
    let mut buffer = vec![0u8; bytes_to_read as usize];
    let mut total_read = 0usize;
    while total_read < buffer.len() {
        let read = file.read(&mut buffer[total_read..]).map_err(|error| {
            command_error(
                "fs_permission_denied",
                operation,
                format!("failed to read log range: {error}"),
            )
        })?;
        if read == 0 {
            break;
        }
        total_read += read;
    }
    let next_byte_offset = from_byte_offset + total_read as u64;
    // Why: re-stat after the read — truncation can race the ranged read on
    // the same open handle, and an offset beyond the new EOF must not become
    // the baseline for later appends (`local-log-tail-reader.ts:52-65`).
    let final_size = file.metadata().map(|m| m.len()).map_err(|error| {
        command_error(
            "fs_permission_denied",
            operation,
            format!("failed to re-stat log: {error}"),
        )
    })?;
    if next_byte_offset > final_size {
        return Ok(LogTailReadResult {
            content_base64: String::new(),
            next_byte_offset: 0,
            file_size: final_size,
            file_identity,
            has_more: final_size > 0,
            reset: true,
        });
    }

    Ok(LogTailReadResult {
        content_base64: crate::fs_download_commands::encode_base64(&buffer[..total_read]),
        next_byte_offset,
        file_size: final_size,
        file_identity,
        has_more: next_byte_offset < final_size,
        reset: false,
    })
}

fn command_error(
    code: &'static str,
    operation: &'static str,
    message: impl Into<String>,
) -> DesktopCommandError {
    DesktopCommandError::new(
        code,
        operation,
        message.into(),
        "MCode could not tail this log. Try again.",
        None,
        None,
    )
}

/// One installed watch: the notify handle lives in the pump thread, so
/// dropping this struct signals the loop and joins it — the synchronous
/// equivalent of `watcher.close()` (`local-log-tail.ts:25-32`).
struct LogTailWatch {
    stop: crossbeam_channel::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LogTailWatch {
    fn drop(&mut self) {
        // Disconnected recv exits the loop; join so teardown is deterministic
        // before the registry forgets the key.
        let _ = self.stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Registry of per-`(windowId, subscriptionId)` watchers
/// (`local-log-tail.ts:18-32`, keyed sender:subscription).
#[derive(Default)]
pub(crate) struct LogTailRegistry {
    watches: Mutex<HashMap<(u64, String), LogTailWatch>>,
}

impl LogTailRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Installs a notify-recommended watcher for `path`. Re-start replaces
    /// the previous watch (`local-log-tail.ts:73` closeWatch before set).
    pub(crate) fn start(
        &self,
        window_id: u64,
        subscription_id: String,
        path: PathBuf,
        emitter: LogTailEmitter,
    ) -> Result<(), mcode_fs::scope::FsError> {
        self.stop(window_id, &subscription_id);
        let (notify_tx, notify_rx) =
            crossbeam_channel::unbounded::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(move |result| {
            let _ = notify_tx.send(result);
        })
        .map_err(|error| mcode_fs::scope::FsError::Io(error.to_string()))?;
        // Why: non-recursive — a log tail watches one file
        // (`watch(filePath, …)`, `local-log-tail.ts:82`).
        watcher
            .watch(&path, notify::RecursiveMode::NonRecursive)
            .map_err(|error| mcode_fs::scope::FsError::Io(error.to_string()))?;

        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let handle = std::thread::Builder::new()
            .name(format!("log-tail-{subscription_id}"))
            .spawn(move || {
                // `_keep_watcher` pins the notify handle; dropping it at
                // thread exit unregisters the watch (WatcherService pattern,
                // watcher.rs:380-383).
                let _keep_watcher = watcher;
                run_watch_loop(&path, &emitter, &notify_rx, &stop_rx);
            })
            .map_err(|error| mcode_fs::scope::FsError::Io(error.to_string()))?;
        self.watches.lock().expect("log tail registry lock").insert(
            (window_id, subscription_id),
            LogTailWatch {
                stop: stop_tx,
                handle: Some(handle),
            },
        );
        Ok(())
    }

    /// Stops one subscription; unknown ids are a no-op (idempotent stop).
    pub(crate) fn stop(&self, window_id: u64, subscription_id: &str) {
        self.watches
            .lock()
            .expect("log tail registry lock")
            .remove(&(window_id, subscription_id.to_owned()));
    }

    /// Releases every watch owned by a destroyed window
    /// (`local-log-tail.ts:34-41` closeSenderWatches).
    pub(crate) fn release_window(&self, window_id: u64) {
        let mut watches = self.watches.lock().expect("log tail registry lock");
        let keys: Vec<(u64, String)> = watches
            .keys()
            .filter(|(owner, _)| *owner == window_id)
            .cloned()
            .collect();
        for key in keys {
            watches.remove(&key);
        }
    }
}

/// Watch loop over notify events (`local-log-tail.ts:82-89`): change events
/// push `change`; the watcher erroring/dying pushes one final `rename` drain
/// then releases the dead handle (`local-log-tail.ts:83-88`).
fn run_watch_loop(
    _path: &Path,
    emitter: &LogTailEmitter,
    notify_rx: &Receiver<notify::Result<notify::Event>>,
    stop_rx: &Receiver<()>,
) {
    loop {
        crossbeam_channel::select! {
            recv(stop_rx) -> _ => return,
            recv(notify_rx) -> result => {
                match result {
                    Ok(Ok(event)) => {
                        // Why: any event on the watched file maps to
                        // `change`; create/remove within the watch scope are
                        // still growth/rotation of the same tail target and
                        // the renderer re-derives identity from the next
                        // read (identity mismatch ⇒ reset).
                        let _ = event;
                        emitter.send("change");
                    }
                    Ok(Err(_)) | Err(_) => {
                        // Why: an error commonly accompanies rotation.
                        // Signal one final drain so the renderer can detect
                        // identity change, then release the dead handle
                        // (`local-log-tail.ts:83-88`).
                        emitter.send("rename");
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_returns_reset_for_stale_identity() {
        let dir = std::env::temp_dir().join(format!("mcode-log-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log.txt");
        std::fs::write(&path, b"hello world").unwrap();
        let result = read_local_log_tail_range(&path, 0, Some("bogus-identity")).unwrap();
        assert!(result.reset);
        assert_eq!(result.next_byte_offset, 0);
        assert_eq!(result.file_size, 11);
        assert!(result.has_more);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_drains_chunks_and_offsets() {
        let dir = std::env::temp_dir().join(format!("mcode-log-tail2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log.txt");
        std::fs::write(&path, b"hello world").unwrap();
        let first = read_local_log_tail_range(&path, 0, None).unwrap();
        assert!(!first.reset);
        assert_eq!(first.next_byte_offset, 11);
        assert!(!first.has_more);
        let decoded = crate::fs_download_commands::decode_base64(&first.content_base64).unwrap();
        assert_eq!(decoded, b"hello world");
        // Offset past EOF → reset.
        let past = read_local_log_tail_range(&path, 50, None).unwrap();
        assert!(past.reset);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_is_not_found_for_missing_files() {
        let missing =
            std::env::temp_dir().join(format!("mcode-log-tail-missing-{}", std::process::id()));
        assert_eq!(
            read_local_log_tail_range(&missing, 0, None)
                .unwrap_err()
                .code,
            "fs_not_found"
        );
    }

    #[test]
    fn read_identity_is_a_stable_colon_triple() {
        let dir = std::env::temp_dir().join(format!("mcode-log-tail4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log.txt");
        std::fs::write(&path, b"x").unwrap();
        let first = read_local_log_tail_range(&path, 0, None).unwrap();
        assert_eq!(first.file_identity.split(':').count(), 3);
        // Same file, same identity.
        let second = read_local_log_tail_range(&path, 0, Some(&first.file_identity)).unwrap();
        assert!(!second.reset);
        assert_eq!(second.file_identity, first.file_identity);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
