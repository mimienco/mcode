//! Chunked download transfers and dialog-backed downloads (spec
//! `docs/migration/FS_CONTRACT_SPEC.md` §4 rows `fs:downloadFile`,
//! `fs:downloadFolder`, `fs:saveDownloadedFile`, `fs:startDownloadedFile`,
//! `fs:appendDownloadedFileChunk`, `fs:finishDownloadedFile`,
//! `fs:cancelDownloadedFile`).
//!
//! Original semantics: `orca/src/main/ipc/filesystem.ts:655-852`,
//! `orca/src/main/ipc/filesystem-download-folder.ts:49-119`, and the
//! helpers at `orca/src/main/ipc/filesystem.ts:205-295`. Split out of
//! `filesystem_commands.rs` because the transfer registry is stateful
//! (`Mutex<HashMap<transferId, session>>`) and the dialog flows bring the
//! `tauri-plugin-dialog` seam into this module.

// Why: this in-flight slice has types not yet consumed by every command; clippy sweep when wiring completes (in-flight download slice: registry/session types are constructed by later commands in this batch).
#![allow(dead_code)]
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::DesktopCommandError;

pub(crate) const FS_DOWNLOAD_FILE_OPERATION: &str = "fs_download_remote_file";
pub(crate) const FS_DOWNLOAD_FOLDER_OPERATION: &str = "fs_download_remote_folder";
pub(crate) const FS_SAVE_DOWNLOADED_FILE_OPERATION: &str = "fs_save_downloaded_file";
pub(crate) const FS_START_DOWNLOAD_OPERATION: &str = "fs_start_download";
pub(crate) const FS_APPEND_DOWNLOAD_CHUNK_OPERATION: &str = "fs_append_download_chunk";
pub(crate) const FS_FINISH_DOWNLOAD_OPERATION: &str = "fs_finish_download";
pub(crate) const FS_CANCEL_DOWNLOAD_OPERATION: &str = "fs_cancel_download";

/// Why: mirrors `DOWNLOAD_SESSION_TTL_MS` (`filesystem.ts:230`); an abandoned
/// transfer must release its temp file even if the renderer never finishes
/// or cancels it.
const DOWNLOAD_SESSION_TTL: Duration = Duration::from_secs(30 * 60);

/// Response shape shared by the download commands (`filesystem.ts:205`
/// `DownloadFileResult`; `filesystem.ts:745-751` adds `transferId` for the
/// chunked start). Serialized manually because the wire tag `canceled` is a
/// **boolean**, which serde's internally-tagged enums cannot express.
#[derive(Debug)]
pub(crate) enum DownloadFileResult {
    Canceled,
    Saved {
        destination_path: String,
    },
    Started {
        transfer_id: String,
        destination_path: String,
    },
}

impl serde::Serialize for DownloadFileResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let (canceled, transfer_id, destination_path) = match self {
            Self::Canceled => (true, None, None),
            Self::Saved { destination_path } => (false, None, Some(destination_path)),
            Self::Started {
                transfer_id,
                destination_path,
            } => (false, Some(transfer_id), Some(destination_path)),
        };
        let mut map = serializer.serialize_map(Some(if transfer_id.is_some() { 3 } else { 2 }))?;
        map.serialize_entry("canceled", &canceled)?;
        if let Some(transfer_id) = transfer_id {
            map.serialize_entry("transferId", transfer_id)?;
        }
        if let Some(destination_path) = destination_path {
            map.serialize_entry("destinationPath", destination_path)?;
        }
        map.end()
    }
}

/// Why: transfer paths stay siblings of the destination because the promote
/// step is a rename, and renames must stay on the destination volume
/// (`filesystem.ts:232-235`).
fn create_sibling_transfer_path(destination_path: &Path, suffix: &str) -> PathBuf {
    let dir = destination_path.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!(".{}.{}", random_transfer_id(), suffix))
}

/// Why: mirrors `randomUUID()` for `transferId` and sibling transfer paths.
/// There is no uuid crate in this workspace (feature-gated out of tauri), so
/// this replicates the `random_runtime_id` pattern from
/// `src-tauri/src/runtime_rpc.rs:441-453`: two `RandomState` draws fold into
/// one SipHash output per 8-byte word, 16 bytes total → 32 hex chars.
fn random_transfer_id() -> String {
    fn next_random_word() -> u64 {
        let a = std::collections::hash_map::RandomState::new();
        let b = std::collections::hash_map::RandomState::new();
        let mut hasher = b.build_hasher();
        hasher.write_u64({
            let mut inner = a.build_hasher();
            inner.write_u64(0x9E37_79B9_7F4A_7C15);
            inner.finish()
        });
        hasher.finish()
    }
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let word = next_random_word().to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Mirrors `sanitizeLocalDownloadFilename`
/// (`orca/src/main/local-download-filename.ts:1-15`): control characters and
/// `<>:"/\|?*` become `_`, trailing dots/spaces are stripped, and Windows
/// reserved device names fall back to `download`.
pub(crate) fn sanitize_local_download_filename(remote_basename: &str) -> String {
    static RESERVED: LazyLock<Vec<String>> = LazyLock::new(|| {
        // `WINDOWS_RESERVED_LOCAL_BASENAME`
        // (`local-download-filename.ts:1-2`): the numeric devices expand to
        // COM1..COM9 / LPT1..LPT9; the superscript variants (¹²³) match via
        // the `com[1-9]` stem check below because `CON.md`-style suffixes
        // split on the first dot.
        let mut names: Vec<String> = ["con", "prn", "aux", "nul", "conin$", "conout$"]
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        for index in 1..=9 {
            names.push(format!("com{index}"));
            names.push(format!("lpt{index}"));
        }
        names
    });
    let sanitized: String = remote_basename
        .chars()
        .map(|c| {
            if (c as u32) < 32 || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = sanitized.trim_end_matches(['.', ' ']);
    // Stem comparison mirrors `WINDOWS_RESERVED_LOCAL_BASENAME` matching the
    // whole name including an optional extension (`CON.md` is reserved).
    let stem = trimmed
        .split_once('.')
        .map_or(trimmed, |(stem, _)| stem)
        .to_ascii_lowercase();
    if trimmed.is_empty() || RESERVED.contains(&stem) {
        return "download".to_owned();
    }
    trimmed.to_owned()
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
        download_user_message_for(operation),
        None,
        None,
    )
}

pub(crate) fn download_user_message_for(operation: &'static str) -> &'static str {
    match operation {
        FS_DOWNLOAD_FILE_OPERATION | FS_DOWNLOAD_FOLDER_OPERATION => {
            "MCode could not download this item from the remote host. Try again."
        }
        FS_SAVE_DOWNLOADED_FILE_OPERATION | FS_START_DOWNLOAD_OPERATION => {
            "MCode could not save this file. Try again."
        }
        FS_APPEND_DOWNLOAD_CHUNK_OPERATION | FS_FINISH_DOWNLOAD_OPERATION => {
            "MCode could not complete this download. Try again."
        }
        _ => "MCode could not complete the download request. Try again.",
    }
}

fn validate_required_string(
    value: &str,
    label: &str,
    operation: &'static str,
) -> Result<(), DesktopCommandError> {
    if value.trim().is_empty() {
        return Err(command_error(
            "fs_invalid_argument",
            operation,
            format!("{label} is required"),
        ));
    }
    Ok(())
}

/// Mirrors `inspectDownloadDestination` (`filesystem.ts:244-257`): a stat
/// hit on a directory is `fs_is_directory`; a hit on a file means the
/// destination existed before this transfer. ENOENT means it did not.
fn inspect_download_destination(
    destination_path: &Path,
    operation: &'static str,
) -> Result<bool, DesktopCommandError> {
    match std::fs::metadata(destination_path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                return Err(command_error(
                    "fs_is_directory",
                    operation,
                    "Cannot download to a directory",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(command_error(
            "fs_permission_denied",
            operation,
            format!("failed to inspect download destination: {error}"),
        )),
    }
}

/// Why: destination was absent when the transfer started; a late appearance
/// means another writer raced this download, and silently overwriting it
/// would corrupt their file (`filesystem.ts:259-269`).
fn assert_destination_still_unclaimed(
    destination_path: &Path,
    operation: &'static str,
) -> Result<(), DesktopCommandError> {
    match std::fs::metadata(destination_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(command_error(
            "fs_permission_denied",
            operation,
            format!("failed to inspect download destination: {error}"),
        )),
        Ok(_) => Err(command_error(
            "fs_destination_race",
            operation,
            "Destination file appeared before download completed",
        )),
    }
}

/// Mirrors `promoteDownloadedFile` (`filesystem.ts:271-295`): an unclaimed
/// destination is a plain rename; an existing destination gets a sibling
/// backup so a failed rename restores the old content instead of losing it.
pub(crate) fn promote_downloaded_file(
    temp_path: &Path,
    destination_path: &Path,
    destination_existed: bool,
    operation: &'static str,
) -> Result<(), DesktopCommandError> {
    if !destination_existed {
        assert_destination_still_unclaimed(destination_path, operation)?;
        return std::fs::rename(temp_path, destination_path).map_err(|error| {
            command_error(
                "fs_permission_denied",
                operation,
                format!("failed to promote download: {error}"),
            )
        });
    }
    let backup_path = create_sibling_transfer_path(destination_path, "backup");
    let mut backup_created = false;
    let result = (|| -> Result<(), DesktopCommandError> {
        std::fs::rename(destination_path, &backup_path).map_err(|error| {
            command_error(
                "fs_permission_denied",
                operation,
                format!("failed to back up destination: {error}"),
            )
        })?;
        backup_created = true;
        // Why: remove-then-rename — Windows `std::fs::rename` refuses to
        // clobber an existing file (task brief for `fs_finish_download`),
        // and the just-created backup already vacated the destination.
        let _ = std::fs::remove_file(destination_path);
        std::fs::rename(temp_path, destination_path).map_err(|error| {
            command_error(
                "fs_permission_denied",
                operation,
                format!("failed to promote download: {error}"),
            )
        })?;
        let _ = std::fs::remove_file(&backup_path);
        Ok(())
    })();
    if result.is_err() && backup_created {
        // Best-effort restore of the previous destination content
        // (`filesystem.ts:290-292`).
        let _ = std::fs::rename(&backup_path, destination_path);
    }
    result
}

fn cleanup_local_transfer_path(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn cleanup_local_transfer_directory(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

struct DownloadSession {
    destination_path: PathBuf,
    temp_path: PathBuf,
    destination_existed: bool,
    handle: Option<File>,
    last_activity: Instant,
}

/// Registry of in-flight chunked downloads keyed by `transferId`
/// (`filesystem.ts:777-784` `downloadSessions` map). `Clone` so commands can
/// move a handle into `spawn_blocking` closures (the `NativeFilesystemState`
/// pattern in `filesystem_commands.rs`).
#[derive(Clone, Default)]
pub(crate) struct DownloadTransferRegistry {
    inner: Arc<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    sessions: Mutex<HashMap<String, DownloadSession>>,
    last_sweep: Mutex<Option<Instant>>,
}

impl DownloadTransferRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Lazy TTL sweep on every new start (`filesystem.ts:771-773` runs an
    /// unref'd per-session timer; Tauri commands have no per-session timer
    /// seam, so the sweep runs at start instead — same bound, coarser edge).
    fn sweep_expired(&self) {
        let mut last_sweep = self.inner.last_sweep.lock().unwrap();
        // Sweep at most once per minute; the 30-min TTL dwarfs the interval.
        if last_sweep.is_some_and(|seen| seen.elapsed() < Duration::from_secs(60)) {
            return;
        }
        *last_sweep = Some(Instant::now());
        drop(last_sweep);
        let mut sessions = self.inner.sessions.lock().unwrap();
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, session)| session.last_activity.elapsed() >= DOWNLOAD_SESSION_TTL)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            if let Some(mut session) = sessions.remove(&id) {
                drop(session.handle.take());
                cleanup_local_transfer_path(&session.temp_path);
            }
        }
    }
}

/// Save dialog for file downloads. Returns `Ok(None)` when canceled
/// (`filesystem.ts:676-680`: dialog cancel is `{canceled:true}`, never an
/// error).
pub(crate) fn blocking_save_file_dialog(
    app: &AppHandle,
    suggested_name: Option<&str>,
) -> Result<Option<PathBuf>, DesktopCommandError> {
    let mut builder = app.dialog().file();
    if let Some(name) = suggested_name {
        builder = builder.set_file_name(name.to_owned());
    }
    match builder.blocking_save_file() {
        Some(picked) => {
            let path = picked.simplified().into_path().map_err(|error| {
                command_error(
                    "fs_invalid_argument",
                    FS_SAVE_DOWNLOADED_FILE_OPERATION,
                    format!("download destination path is not usable: {error}"),
                )
            })?;
            Ok(Some(path))
        }
        None => Ok(None),
    }
}

/// Open-folder dialog for folder downloads
/// (`filesystem-download-folder.ts:79-88`).
pub(crate) fn blocking_pick_folder_dialog(
    app: &AppHandle,
) -> Result<Option<PathBuf>, DesktopCommandError> {
    match app.dialog().file().blocking_pick_folder() {
        Some(picked) => {
            let path = picked.simplified().into_path().map_err(|error| {
                command_error(
                    "fs_invalid_argument",
                    FS_DOWNLOAD_FOLDER_OPERATION,
                    format!("download destination path is not usable: {error}"),
                )
            })?;
            Ok(Some(path))
        }
        None => Ok(None),
    }
}

/// `fs_download_remote_file` (spec §4 `fs:downloadFile`,
/// `filesystem.ts:655-698`).
///
/// The original is SSH-only: the download transport lives in the SSH
/// provider (`requireSshFilesystemProvider`, `filesystem.ts:663`), so the
/// Tauri port rejects *any* remote download with `fs_remote_unsupported`
/// BEFORE any dialog (task brief: no dialog side effects on an unsupported
/// path). The `mcode-ssh` fs-dispatch slice is the open parity row; when it
/// lands, the SSH branch resumes at `filesystem.ts:664-695`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_download_remote_file(
    file_path: Option<String>,
    connection_id: Option<String>,
) -> Result<DownloadFileResult, DesktopCommandError> {
    let operation = FS_DOWNLOAD_FILE_OPERATION;
    // Validate argument shapes first so a bad call reports the same
    // `fs_invalid_argument` the original's validateRequiredString produced
    // (`filesystem.ts:661-662`), then hit the remote-unsupported guard.
    let file_path = file_path.unwrap_or_default();
    let connection_id = connection_id.unwrap_or_default();
    validate_required_string(&file_path, "filePath", operation)?;
    validate_required_string(&connection_id, "connectionId", operation)?;
    Err(command_error(
        "fs_remote_unsupported",
        operation,
        "Remote file download is unavailable. Reconnect the SSH target and retry.",
    ))
}

/// `fs_download_remote_folder` (spec §4 `fs:downloadFolder`,
/// `filesystem-download-folder.ts:49-119`). Same SSH-only restriction as the
/// file variant; `fs_remote_unsupported` before any picker
/// (`filesystem-download-folder.ts:58-63`).
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_download_remote_folder(
    dir_path: Option<String>,
    connection_id: Option<String>,
) -> Result<DownloadFileResult, DesktopCommandError> {
    let operation = FS_DOWNLOAD_FOLDER_OPERATION;
    let dir_path = dir_path.unwrap_or_default();
    let connection_id = connection_id.unwrap_or_default();
    validate_required_string(&dir_path, "dirPath", operation)?;
    validate_required_string(&connection_id, "connectionId", operation)?;
    Err(command_error(
        "fs_remote_unsupported",
        operation,
        "Remote folder download is unavailable. Reconnect the SSH target and retry.",
    ))
}

/// `fs_save_downloaded_file` (spec §4 `fs:saveDownloadedFile`,
/// `filesystem.ts:701-738`): sanitize the suggested name, decode utf8 or
/// base64 content, write through the sibling-temp + promote path.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_save_downloaded_file(
    app: AppHandle,
    suggested_name: Option<String>,
    content: Option<String>,
    encoding: Option<String>,
) -> Result<DownloadFileResult, DesktopCommandError> {
    let operation = FS_SAVE_DOWNLOADED_FILE_OPERATION;
    let suggested_name = suggested_name.unwrap_or_default();
    validate_required_string(&suggested_name, "suggestedName", operation)?;
    let content = match content {
        Some(content) => content,
        None => {
            return Err(command_error(
                "fs_invalid_argument",
                operation,
                "content is required",
            ));
        }
    };
    let sanitized = sanitize_local_download_filename(&suggested_name);
    // Decode off the async runtime: base64 payloads reach 50 MB.
    let bytes =
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, DesktopCommandError> {
            if encoding.as_deref() == Some("base64") {
                decode_base64(&content).ok_or_else(|| {
                    command_error(
                        "fs_invalid_argument",
                        operation,
                        "content is not valid base64",
                    )
                })
            } else {
                Ok(content.into_bytes())
            }
        })
        .await
        .map_err(|error| {
            command_error(
                "fs_blocking_task_failed",
                operation,
                format!("download task failed: {error}"),
            )
        })??;

    let saved = tauri::async_runtime::spawn_blocking(move || {
        let Some(destination_path) = blocking_save_file_dialog(&app, Some(&sanitized))? else {
            return Ok(DownloadFileResult::Canceled);
        };
        let existed = inspect_download_destination(&destination_path, operation)?;
        let temp_path = create_sibling_transfer_path(&destination_path, "download");
        if let Err(error) = std::fs::write(&temp_path, &bytes) {
            cleanup_local_transfer_path(&temp_path);
            return Err(command_error(
                "fs_permission_denied",
                operation,
                format!("failed to write download: {error}"),
            ));
        }
        match promote_downloaded_file(&temp_path, &destination_path, existed, operation) {
            Ok(()) => Ok(DownloadFileResult::Saved {
                destination_path: destination_path.to_string_lossy().into_owned(),
            }),
            Err(error) => {
                cleanup_local_transfer_path(&temp_path);
                Err(error)
            }
        }
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("download task failed: {error}"),
        )
    })??;
    Ok(saved)
}

/// `fs_start_download` (spec §4 `fs:startDownloadedFile`,
/// `filesystem.ts:740-798`): save dialog FIRST (contract), then open a
/// sibling temp file with `wx`, register the session under a fresh
/// `transferId`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_start_download(
    app: AppHandle,
    registry: tauri::State<'_, DownloadTransferRegistry>,
    suggested_name: Option<String>,
) -> Result<DownloadFileResult, DesktopCommandError> {
    let operation = FS_START_DOWNLOAD_OPERATION;
    let suggested_name = suggested_name.unwrap_or_default();
    validate_required_string(&suggested_name, "suggestedName", operation)?;
    registry.sweep_expired();

    let registry = registry.inner().clone();
    let suggested_name = sanitize_local_download_filename(&suggested_name);
    let started = tauri::async_runtime::spawn_blocking(move || {
        // Why: dialog-before-work is the contract; a canceled dialog must
        // never leave a temp file behind (filesystem.ts:756-762).
        let Some(destination_path) = blocking_save_file_dialog(&app, Some(&suggested_name))? else {
            return Ok(DownloadFileResult::Canceled);
        };
        let existed = inspect_download_destination(&destination_path, operation)?;
        let temp_path = create_sibling_transfer_path(&destination_path, "download");
        // 'wx': create-new semantics so a reused temp name can never clobber
        // (filesystem.ts:769).
        let handle = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| {
                command_error(
                    "fs_permission_denied",
                    operation,
                    format!("failed to open download temp file: {error}"),
                )
            })?;
        let transfer_id = random_transfer_id();
        let destination = destination_path.to_string_lossy().into_owned();
        registry.inner.sessions.lock().unwrap().insert(
            transfer_id.clone(),
            DownloadSession {
                destination_path,
                temp_path,
                destination_existed: existed,
                handle: Some(handle),
                last_activity: Instant::now(),
            },
        );
        Ok(DownloadFileResult::Started {
            transfer_id,
            destination_path: destination,
        })
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("download task failed: {error}"),
        )
    })??;
    Ok(started)
}

/// `fs_append_download_chunk` (spec §4 `fs:appendDownloadedFileChunk`,
/// `filesystem.ts:800-815`): unknown transfer → `fs_transfer_unknown`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_append_download_chunk(
    registry: tauri::State<'_, DownloadTransferRegistry>,
    transfer_id: String,
    content_base64: String,
) -> Result<serde_json::Value, DesktopCommandError> {
    let operation = FS_APPEND_DOWNLOAD_CHUNK_OPERATION;
    validate_required_string(&transfer_id, "transferId", operation)?;
    validate_required_string(&content_base64, "contentBase64", operation)?;
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let decoded = decode_base64(&content_base64).ok_or_else(|| {
            command_error(
                "fs_invalid_argument",
                operation,
                "contentBase64 is not valid base64",
            )
        })?;
        let mut sessions = registry.inner.sessions.lock().unwrap();
        let Some(session) = sessions.get_mut(&transfer_id) else {
            return Err(command_error(
                "fs_transfer_unknown",
                operation,
                "Download session not found",
            ));
        };
        let Some(handle) = session.handle.as_mut() else {
            return Err(command_error(
                "fs_transfer_unknown",
                operation,
                "Download session not found",
            ));
        };
        handle.write_all(&decoded).map_err(|error| {
            command_error(
                "fs_permission_denied",
                operation,
                format!("failed to append download chunk: {error}"),
            )
        })?;
        session.last_activity = Instant::now();
        Ok(serde_json::json!({ "ok": true }))
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("download task failed: {error}"),
        )
    })?
}

/// `fs_finish_download` (spec §4 `fs:finishDownloadedFile`,
/// `filesystem.ts:817-843`): close the handle, promote temp → destination.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_finish_download(
    registry: tauri::State<'_, DownloadTransferRegistry>,
    transfer_id: String,
) -> Result<DownloadFileResult, DesktopCommandError> {
    let operation = FS_FINISH_DOWNLOAD_OPERATION;
    validate_required_string(&transfer_id, "transferId", operation)?;
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(mut session) = registry.inner.sessions.lock().unwrap().remove(&transfer_id) else {
            return Err(command_error(
                "fs_transfer_unknown",
                operation,
                "Download session not found",
            ));
        };
        drop(session.handle.take());
        let temp_path = session.temp_path.clone();
        let destination_path = session.destination_path.clone();
        let existed = session.destination_existed;
        match promote_downloaded_file(&temp_path, &destination_path, existed, operation) {
            Ok(()) => Ok(DownloadFileResult::Saved {
                destination_path: destination_path.to_string_lossy().into_owned(),
            }),
            Err(error) => {
                cleanup_local_transfer_path(&temp_path);
                Err(error)
            }
        }
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("download task failed: {error}"),
        )
    })?
}

/// `fs_cancel_download` (spec §4 `fs:cancelDownloadedFile`,
/// `filesystem.ts:845-852`): close + discard; idempotent `Ok`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_cancel_download(
    registry: tauri::State<'_, DownloadTransferRegistry>,
    transfer_id: String,
) -> Result<serde_json::Value, DesktopCommandError> {
    let operation = FS_CANCEL_DOWNLOAD_OPERATION;
    validate_required_string(&transfer_id, "transferId", operation)?;
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(mut session) = registry.inner.sessions.lock().unwrap().remove(&transfer_id) {
            drop(session.handle.take());
            cleanup_local_transfer_path(&session.temp_path);
        }
        Ok(serde_json::json!({ "ok": true }))
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("download task failed: {error}"),
        )
    })?
}

/// Whitespace-tolerant standard-base64 decoder matching Node's
/// `Buffer.from(str, 'base64')` for the shapes the renderer produces
/// (`decodeDownloadedFileContent`, `filesystem.ts:214-219`; the chunk path
/// uses the same alphabet). Hand-rolled because no base64 crate is a
/// workspace dependency.
pub(crate) fn decode_base64(value: &str) -> Option<Vec<u8>> {
    const INVALID: u8 = 0xFF;
    static TABLE: LazyLock<[u8; 256]> = LazyLock::new(|| {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut table = [INVALID; 256];
        for (index, &byte) in alphabet.iter().enumerate() {
            table[byte as usize] = index as u8;
        }
        table
    });
    let table = &*TABLE;
    let bytes: Vec<u8> = value
        .bytes()
        .filter(|b| !matches!(b, b'\r' | b'\n' | b' ' | b'\t'))
        .collect();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut quad = [0u8; 4];
    let mut quad_len = 0usize;
    let mut padding = 0usize;
    for &b in &bytes {
        if b == b'=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            return None; // data after padding
        }
        let v = table[b as usize];
        if v == INVALID {
            return None;
        }
        quad[quad_len] = v;
        quad_len += 1;
        if quad_len == 4 {
            out.push((quad[0] << 2) | (quad[1] >> 4));
            out.push((quad[1] << 4) | (quad[2] >> 2));
            out.push((quad[2] << 6) | quad[3]);
            quad_len = 0;
        }
    }
    match quad_len {
        0 => {}
        2 => out.push((quad[0] << 2) | (quad[1] >> 4)),
        3 => {
            out.push((quad[0] << 2) | (quad[1] >> 4));
            out.push((quad[1] << 4) | (quad[2] >> 2));
        }
        _ => return None,
    }
    Some(out)
}

/// Standard-base64 encoder used by the log-tail read and upload staging
/// (the renderer decodes with Node's `Buffer.from(str, 'base64')`, whose
/// default alphabet this matches). Hand-rolled because no base64 crate is a
/// src-tauri dependency (mcode-fs has one, but the module boundary stays
/// inside src-tauri).
pub(crate) fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3F] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_matches_original_reserved_and_char_rules() {
        assert_eq!(sanitize_local_download_filename("report.md"), "report.md");
        assert_eq!(sanitize_local_download_filename("a<b>:c?.md"), "a_b__c_.md");
        assert_eq!(sanitize_local_download_filename("trailing... "), "trailing");
        assert_eq!(sanitize_local_download_filename("con"), "download");
        assert_eq!(sanitize_local_download_filename("CON.md"), "download");
        assert_eq!(sanitize_local_download_filename(""), "download");
        assert_eq!(sanitize_local_download_filename("nul"), "download");
        assert_eq!(sanitize_local_download_filename("lpt3.txt"), "download");
    }

    #[test]
    fn transfer_ids_are_unique_32_hex_chars() {
        let a = random_transfer_id();
        let b = random_transfer_id();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn base64_decode_round_trips() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVs\r\nbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGk=").unwrap(), b"hi");
        assert_eq!(decode_base64("YQ==").unwrap(), b"a");
        assert_eq!(decode_base64(""), Some(Vec::new()));
        assert_eq!(decode_base64("!!!!"), None);
    }

    #[test]
    fn download_result_serializes_the_renderer_wire_shape() {
        assert_eq!(
            serde_json::to_value(DownloadFileResult::Canceled).unwrap(),
            serde_json::json!({ "canceled": true })
        );
        assert_eq!(
            serde_json::to_value(DownloadFileResult::Saved {
                destination_path: "D:/out/x.bin".to_owned()
            })
            .unwrap(),
            serde_json::json!({ "canceled": false, "destinationPath": "D:/out/x.bin" })
        );
        assert_eq!(
            serde_json::to_value(DownloadFileResult::Started {
                transfer_id: "abc".to_owned(),
                destination_path: "D:/out/x.bin".to_owned()
            })
            .unwrap(),
            serde_json::json!({
                "canceled": false,
                "transferId": "abc",
                "destinationPath": "D:/out/x.bin"
            })
        );
    }

    #[test]
    fn destination_race_and_directory_checks_match_original_codes() {
        let dir = std::env::temp_dir().join(format!("mcode-fs-dl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("existing.txt");
        std::fs::write(&file, b"x").unwrap();
        // Existing file → existed=true (no error).
        assert!(inspect_download_destination(&file, FS_SAVE_DOWNLOADED_FILE_OPERATION).unwrap());
        // Directory → fs_is_directory.
        assert_eq!(
            inspect_download_destination(&dir, FS_SAVE_DOWNLOADED_FILE_OPERATION)
                .unwrap_err()
                .code,
            "fs_is_directory"
        );
        // Unclaimed destination that later appears → fs_destination_race.
        let missing = dir.join("missing.txt");
        assert!(
            !inspect_download_destination(&missing, FS_SAVE_DOWNLOADED_FILE_OPERATION).unwrap()
        );
        std::fs::write(&missing, b"x").unwrap();
        assert_eq!(
            assert_destination_still_unclaimed(&missing, FS_FINISH_DOWNLOAD_OPERATION)
                .unwrap_err()
                .code,
            "fs_destination_race"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn promote_backs_up_and_restores_on_failure() {
        let dir = std::env::temp_dir().join(format!("mcode-fs-promote-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("dest.txt");
        std::fs::write(&dest, b"old").unwrap();
        let temp = dir.join("temp.txt");
        std::fs::write(&temp, b"new").unwrap();
        promote_downloaded_file(&temp, &dest, true, FS_FINISH_DOWNLOAD_OPERATION).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        // Temp consumed by rename.
        assert!(!temp.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finish_promotes_unclaimed_destination_and_enforces_no_race() {
        let registry = DownloadTransferRegistry::new();
        let dir = std::env::temp_dir().join(format!("mcode-fs-finish-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("dest2.txt");
        let temp = dir.join("temp2.txt");
        std::fs::write(&temp, b"data").unwrap();
        registry.inner.sessions.lock().unwrap().insert(
            "t1".to_owned(),
            DownloadSession {
                destination_path: dest.clone(),
                temp_path: temp.clone(),
                destination_existed: false,
                handle: None,
                last_activity: Instant::now(),
            },
        );
        // Race: destination appeared after the transfer started.
        std::fs::write(&dest, b"rival").unwrap();
        let err = {
            let mut sessions = registry.inner.sessions.lock().unwrap();
            let session = sessions.remove("t1").unwrap();
            promote_downloaded_file(
                &session.temp_path,
                &session.destination_path,
                session.destination_existed,
                FS_FINISH_DOWNLOAD_OPERATION,
            )
            .unwrap_err()
        };
        assert_eq!(err.code, "fs_destination_race");
        assert_eq!(std::fs::read(&dest).unwrap(), b"rival");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
