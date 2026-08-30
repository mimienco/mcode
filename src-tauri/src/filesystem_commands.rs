use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use mcode_fs::{
    BoundedFileRead, DirectoryEntry, FsError, PathKind, PathMetadata, ScopedFilesystem,
};
use mcode_settings::{SettingsService, SettingsServiceError};
use serde::Serialize;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FS_READ_DIR_OPERATION: &str = "fs_read_dir";
const FS_READ_FILE_OPERATION: &str = "fs_read_file";
const FS_WRITE_FILE_ATOMIC_OPERATION: &str = "fs_write_file_atomic";
const FS_STAT_OPERATION: &str = "fs_stat";
const FS_PATH_EXISTS_OPERATION: &str = "fs_path_exists";
const MAX_FILE_CONTENT_BYTES: usize = 50 * 1024 * 1024;
const BINARY_PROBE_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct NativeFilesystemState {
    filesystem: ScopedFilesystem,
    settings: SettingsService,
    cached_roots: Arc<Mutex<Option<Vec<PathBuf>>>>,
}

impl NativeFilesystemState {
    pub(crate) fn new(filesystem: ScopedFilesystem, settings: SettingsService) -> Self {
        Self {
            filesystem,
            settings,
            cached_roots: Arc::new(Mutex::new(None)),
        }
    }

    fn refresh_roots_if_changed(
        &self,
        operation: &'static str,
    ) -> Result<(), FilesystemCommandError> {
        self.refresh_roots_with(operation, || {
            self.settings
                .local_filesystem_roots()
                .map_err(|error| FilesystemCommandError::settings(error, operation))
        })
    }

    fn refresh_roots_with(
        &self,
        operation: &'static str,
        load_roots: impl FnOnce() -> Result<Vec<PathBuf>, FilesystemCommandError>,
    ) -> Result<(), FilesystemCommandError> {
        let mut cached_roots = self.cached_roots.lock().map_err(|_| {
            FilesystemCommandError::new(
                "fs_scope_refresh_unavailable",
                operation,
                "filesystem scope refresh state is unavailable",
            )
        })?;
        // Reading settings, comparing the cached snapshot, and replacing live
        // capabilities are one serialized transaction. Reading before this lock
        // would let a delayed request reinstall roots that a newer request revoked.
        let roots = load_roots()?;

        if cached_roots.as_ref() == Some(&roots) {
            return Ok(());
        }
        let registered_roots = self
            .filesystem
            .replace_roots(roots.iter())
            .map_err(|error| FilesystemCommandError::filesystem(error, operation))?;
        *cached_roots = (registered_roots.len() == roots.len()).then_some(roots);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsDirectoryEntry {
    name: String,
    is_directory: bool,
    is_symlink: bool,
}

impl From<DirectoryEntry> for FsDirectoryEntry {
    fn from(entry: DirectoryEntry) -> Self {
        Self {
            name: entry.name,
            is_directory: entry.kind == PathKind::Directory,
            is_symlink: entry.kind == PathKind::Symlink,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsPathMetadata {
    size: u64,
    is_directory: bool,
    mtime: f64,
}

impl From<PathMetadata> for FsPathMetadata {
    fn from(metadata: PathMetadata) -> Self {
        Self {
            size: metadata.size,
            is_directory: metadata.kind == PathKind::Directory,
            mtime: metadata.modified_time_millis,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsFileContent {
    content: String,
    is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_image: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_identity: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FilesystemCommandError {
    code: &'static str,
    context: FilesystemCommandErrorContext,
    message: String,
    user_message: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct FilesystemCommandErrorContext {
    operation: &'static str,
}

impl FilesystemCommandError {
    fn new(code: &'static str, operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            context: FilesystemCommandErrorContext { operation },
            message: message.into(),
            user_message: user_message_for(operation),
        }
    }

    fn filesystem(error: FsError, operation: &'static str) -> Self {
        let code = match &error {
            FsError::PathOutsideScope { .. } | FsError::SymlinkTraversalDenied { .. } => {
                "fs_path_denied"
            }
            FsError::InvalidTargetPath { .. } | FsError::InvalidTrustedRoot { .. } => {
                "fs_invalid_argument"
            }
            FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                "fs_not_found"
            }
            FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::PermissionDenied => {
                "fs_permission_denied"
            }
            FsError::Io { .. } => "fs_io",
            _ => error.code(),
        };
        Self::new(code, operation, error.to_string())
    }

    fn settings(error: SettingsServiceError, operation: &'static str) -> Self {
        Self::new(error.code(), operation, error.to_string())
    }

    fn blocking_task(error: impl std::fmt::Display, operation: &'static str) -> Self {
        Self::new(
            "fs_blocking_task_failed",
            operation,
            format!("filesystem task failed: {error}"),
        )
    }
}

fn user_message_for(operation: &'static str) -> &'static str {
    match operation {
        FS_READ_DIR_OPERATION => "MCode could not list this folder. Try again.",
        FS_READ_FILE_OPERATION => "MCode could not read this file. Try again.",
        FS_WRITE_FILE_ATOMIC_OPERATION => "MCode could not save this file. Try again.",
        FS_STAT_OPERATION | FS_PATH_EXISTS_OPERATION => {
            "MCode could not inspect this path. Try again."
        }
        _ => "MCode could not complete the filesystem request. Try again.",
    }
}

fn previewable_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" | "jfif" | "jpe" => Some("image/jpeg"),
        "avif" => Some("image/avif"),
        "gif" => Some("image/gif"),
        "svg" => Some("image/svg+xml"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

fn read_file_content(
    filesystem: &ScopedFilesystem,
    file_path: &Path,
    include_file_identity: bool,
) -> Result<FsFileContent, FsError> {
    let mime_type = previewable_mime_type(file_path);
    let probe_bytes = mime_type.is_none().then_some(BINARY_PROBE_BYTES);
    let snapshot =
        filesystem.read_file_snapshot_bounded(file_path, MAX_FILE_CONTENT_BYTES, probe_bytes)?;
    let file_identity = include_file_identity.then_some(snapshot.file_identity);
    match snapshot.read {
        BoundedFileRead::BinaryPrefix => Ok(FsFileContent {
            content: String::new(),
            is_binary: true,
            is_image: None,
            mime_type: None,
            file_identity,
        }),
        BoundedFileRead::Bytes(bytes) if mime_type.is_some() => Ok(FsFileContent {
            content: BASE64_STANDARD.encode(bytes),
            is_binary: true,
            is_image: Some(true),
            mime_type,
            file_identity,
        }),
        BoundedFileRead::Bytes(bytes) => Ok(FsFileContent {
            content: String::from_utf8_lossy(&bytes).into_owned(),
            is_binary: false,
            is_image: None,
            mime_type: None,
            file_identity,
        }),
    }
}

fn local_path(
    value: String,
    connection_id: Option<String>,
    operation: &'static str,
) -> Result<PathBuf, FilesystemCommandError> {
    if connection_id.is_some() {
        return Err(FilesystemCommandError::new(
            "fs_remote_unsupported",
            operation,
            "remote filesystem requests must use the owning runtime transport",
        ));
    }
    if value.is_empty() || value.contains('\0') {
        return Err(FilesystemCommandError::new(
            "fs_invalid_argument",
            operation,
            "filesystem path must be a non-empty absolute path",
        ));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(FilesystemCommandError::new(
            "fs_invalid_argument",
            operation,
            "filesystem path must be absolute",
        ));
    }
    Ok(path)
}

async fn run_filesystem_task<T: Send + 'static>(
    state: NativeFilesystemState,
    operation: &'static str,
    task: impl FnOnce(&ScopedFilesystem) -> Result<T, FsError> + Send + 'static,
) -> Result<T, FilesystemCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = state.refresh_roots_if_changed(operation).and_then(|()| {
            task(&state.filesystem)
                .map_err(|error| FilesystemCommandError::filesystem(error, operation))
        });
        #[cfg(debug_assertions)]
        if let Err(error) = &result {
            eprintln!("[filesystem] {operation} failed: {}", error.message);
        }
        result
    })
    .await
    .map_err(|error| FilesystemCommandError::blocking_task(error, operation))?
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_read_dir(
    state: tauri::State<'_, NativeFilesystemState>,
    dir_path: String,
    connection_id: Option<String>,
) -> Result<Vec<FsDirectoryEntry>, FilesystemCommandError> {
    let dir_path = local_path(dir_path, connection_id, FS_READ_DIR_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_READ_DIR_OPERATION,
        move |filesystem| {
            filesystem
                .read_dir(Path::new(&dir_path))
                .map(|entries| entries.into_iter().map(FsDirectoryEntry::from).collect())
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_read_file(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
    include_local_log_metadata: Option<bool>,
) -> Result<FsFileContent, FilesystemCommandError> {
    let file_path = local_path(file_path, connection_id, FS_READ_FILE_OPERATION)?;
    let include_file_identity = include_local_log_metadata == Some(true);
    run_filesystem_task(
        state.inner().clone(),
        FS_READ_FILE_OPERATION,
        move |filesystem| read_file_content(filesystem, &file_path, include_file_identity),
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_write_file_atomic(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    content: String,
    connection_id: Option<String>,
) -> Result<(), FilesystemCommandError> {
    let file_path = local_path(file_path, connection_id, FS_WRITE_FILE_ATOMIC_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_WRITE_FILE_ATOMIC_OPERATION,
        move |filesystem| {
            filesystem.write_file_atomic(&file_path, content.as_bytes(), MAX_FILE_CONTENT_BYTES)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_stat(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
) -> Result<FsPathMetadata, FilesystemCommandError> {
    let file_path = local_path(file_path, connection_id, FS_STAT_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_STAT_OPERATION,
        move |filesystem| {
            filesystem
                .stat(Path::new(&file_path))
                .map(FsPathMetadata::from)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_path_exists(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
) -> Result<bool, FilesystemCommandError> {
    let file_path = local_path(file_path, connection_id, FS_PATH_EXISTS_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_PATH_EXISTS_OPERATION,
        move |filesystem| filesystem.path_exists(Path::new(&file_path)),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mcode-tauri-fs-state-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_repo_root(settings_path: &Path, id: &str, root: &Path) {
        fs::write(
            settings_path,
            serde_json::to_vec(&json!({
                "repos": [{ "id": id, "path": root.to_string_lossy() }]
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn directory_entries_serialize_the_electron_compatible_shape() {
        let entry = FsDirectoryEntry::from(DirectoryEntry {
            name: "docs".to_owned(),
            kind: PathKind::Directory,
        });

        assert_eq!(
            serde_json::to_value(entry).unwrap(),
            json!({ "name": "docs", "isDirectory": true, "isSymlink": false })
        );
    }

    #[test]
    fn metadata_serializes_the_electron_compatible_shape() {
        let metadata = FsPathMetadata::from(PathMetadata {
            kind: PathKind::File,
            size: 42,
            modified_time_millis: 1_725_000_000_123.0,
        });

        assert_eq!(
            serde_json::to_value(metadata).unwrap(),
            json!({ "size": 42, "isDirectory": false, "mtime": 1_725_000_000_123.0 })
        );
    }

    #[test]
    fn file_content_preserves_electron_text_binary_and_preview_shapes() {
        let fixture = fixture_path();
        fs::create_dir_all(&fixture).unwrap();
        let text = fixture.join("notes.txt");
        let binary = fixture.join("archive.bin");
        let image = fixture.join("pixel.png");
        fs::write(&text, "hello 🌍").unwrap();
        fs::write(&binary, [b'a', 0, b'b']).unwrap();
        fs::write(&image, [0, 1, 2]).unwrap();
        let filesystem = ScopedFilesystem::new();
        filesystem.register_root(&fixture).unwrap();

        assert_eq!(
            serde_json::to_value(read_file_content(&filesystem, &text, false).unwrap()).unwrap(),
            json!({ "content": "hello 🌍", "isBinary": false })
        );
        assert_eq!(
            serde_json::to_value(read_file_content(&filesystem, &binary, false).unwrap()).unwrap(),
            json!({ "content": "", "isBinary": true })
        );
        assert_eq!(
            serde_json::to_value(read_file_content(&filesystem, &image, false).unwrap()).unwrap(),
            json!({
                "content": "AAEC",
                "isBinary": true,
                "isImage": true,
                "mimeType": "image/png"
            })
        );

        drop(filesystem);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn filesystem_limits_and_target_errors_keep_stable_adapter_codes() {
        let too_large = FsError::FileTooLarge {
            path: PathBuf::from("large.log"),
            size: 11,
            limit: 10,
        };
        let directory = FsError::TargetIsDirectory {
            path: PathBuf::from("folder"),
        };

        assert_eq!(
            FilesystemCommandError::filesystem(too_large, FS_READ_FILE_OPERATION).code,
            "fs_file_too_large"
        );
        assert_eq!(
            FilesystemCommandError::filesystem(directory, FS_WRITE_FILE_ATOMIC_OPERATION).code,
            "fs_target_is_directory"
        );
    }

    #[test]
    fn local_log_metadata_requests_include_a_stable_non_empty_file_identity() {
        let fixture = fixture_path();
        fs::create_dir_all(&fixture).unwrap();
        let log = fixture.join("live.log");
        fs::write(&log, "first").unwrap();
        let filesystem = ScopedFilesystem::new();
        filesystem.register_root(&fixture).unwrap();

        let first = read_file_content(&filesystem, &log, true).unwrap();
        fs::write(&log, "first\nsecond").unwrap();
        let appended = read_file_content(&filesystem, &log, true).unwrap();

        assert!(
            first
                .file_identity
                .as_ref()
                .is_some_and(|value| !value.is_empty())
        );
        assert_eq!(first.file_identity, appended.file_identity);

        drop(filesystem);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn errors_include_a_stable_code_operation_and_safe_message() {
        let error = FilesystemCommandError::new(
            "fs_path_denied",
            FS_READ_DIR_OPERATION,
            "path is outside every trusted root",
        );

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "fs_path_denied",
                "context": { "operation": "fs_read_dir" },
                "message": "path is outside every trusted root",
                "userMessage": "MCode could not list this folder. Try again."
            })
        );
    }

    #[test]
    fn capability_boundary_errors_share_the_public_denial_code() {
        let outside = FilesystemCommandError::filesystem(
            FsError::PathOutsideScope {
                path: PathBuf::from("outside"),
            },
            FS_STAT_OPERATION,
        );
        let symlink = FilesystemCommandError::filesystem(
            FsError::SymlinkTraversalDenied {
                path: PathBuf::from("link"),
            },
            FS_STAT_OPERATION,
        );

        assert_eq!(outside.code, "fs_path_denied");
        assert_eq!(symlink.code, "fs_path_denied");
    }

    #[test]
    fn request_paths_must_be_absolute_local_paths() {
        let absolute_path = std::env::current_dir()
            .unwrap()
            .join("file.ts")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            local_path("relative/file.ts".to_owned(), None, FS_STAT_OPERATION)
                .unwrap_err()
                .code,
            "fs_invalid_argument"
        );
        assert_eq!(
            local_path(
                absolute_path.clone(),
                Some("ssh-1".to_owned()),
                FS_STAT_OPERATION
            )
            .unwrap_err()
            .code,
            "fs_remote_unsupported"
        );
        assert_eq!(
            local_path(absolute_path, Some(String::new()), FS_STAT_OPERATION)
                .unwrap_err()
                .code,
            "fs_remote_unsupported"
        );
    }

    #[test]
    fn concurrent_refresh_loads_snapshots_inside_the_serialized_transaction() {
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        let fixture = fixture_path();
        let old_root = fixture.join("old");
        let new_root = fixture.join("new");
        fs::create_dir_all(&old_root).unwrap();
        fs::create_dir_all(&new_root).unwrap();
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(fixture.join("unused-settings.json")),
        );
        let (old_loaded_tx, old_loaded_rx) = mpsc::channel();
        let (release_old_tx, release_old_rx) = mpsc::channel();
        let (new_loaded_tx, new_loaded_rx) = mpsc::channel();

        let old_state = state.clone();
        let old_root_for_thread = old_root.clone();
        let old_refresh = thread::spawn(move || {
            old_state.refresh_roots_with(FS_STAT_OPERATION, || {
                old_loaded_tx.send(()).unwrap();
                release_old_rx.recv().unwrap();
                Ok(vec![old_root_for_thread])
            })
        });
        old_loaded_rx.recv().unwrap();

        let new_state = state.clone();
        let new_root_for_thread = new_root.clone();
        let new_refresh = thread::spawn(move || {
            new_state.refresh_roots_with(FS_STAT_OPERATION, || {
                new_loaded_tx.send(()).unwrap();
                Ok(vec![new_root_for_thread])
            })
        });
        assert!(
            new_loaded_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );
        release_old_tx.send(()).unwrap();
        old_refresh.join().unwrap().unwrap();
        new_refresh.join().unwrap().unwrap();
        new_loaded_rx.recv().unwrap();

        assert!(matches!(
            state.filesystem.stat(&old_root),
            Err(FsError::PathOutsideScope { .. })
        ));
        assert!(state.filesystem.stat(&new_root).is_ok());
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn changed_root_set_revokes_removed_roots_atomically() {
        let fixture = fixture_path();
        let first_root = fixture.join("first");
        let replacement_root = fixture.join("replacement-with-a-different-length");
        fs::create_dir_all(&first_root).unwrap();
        fs::create_dir_all(&replacement_root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "first", &first_root);
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(settings_path.clone()),
        );

        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();
        assert!(state.filesystem.stat(&first_root).is_ok());

        write_repo_root(&settings_path, "replacement", &replacement_root);
        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();

        assert!(matches!(
            state.filesystem.stat(&first_root),
            Err(FsError::PathOutsideScope { .. })
        ));
        assert!(state.filesystem.stat(&replacement_root).is_ok());
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn file_identity_detects_same_length_replacement_with_preserved_modified_time() {
        let fixture = fixture_path();
        let first_root = fixture.join("first");
        let next_root = fixture.join("other");
        fs::create_dir_all(&first_root).unwrap();
        fs::create_dir_all(&next_root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "scope", &first_root);
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(settings_path.clone()),
        );
        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();
        let original_metadata = fs::metadata(&settings_path).unwrap();

        let replacement = fixture.join("replacement.json");
        write_repo_root(&replacement, "scope", &next_root);
        let replacement_file = fs::OpenOptions::new()
            .write(true)
            .open(&replacement)
            .unwrap();
        replacement_file
            .set_times(fs::FileTimes::new().set_modified(original_metadata.modified().unwrap()))
            .unwrap();
        drop(replacement_file);
        fs::remove_file(&settings_path).unwrap();
        fs::rename(&replacement, &settings_path).unwrap();
        let replacement_metadata = fs::metadata(&settings_path).unwrap();
        assert_eq!(replacement_metadata.len(), original_metadata.len());
        assert_eq!(
            replacement_metadata.modified().unwrap(),
            original_metadata.modified().unwrap()
        );

        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();

        assert!(matches!(
            state.filesystem.stat(&first_root),
            Err(FsError::PathOutsideScope { .. })
        ));
        assert!(state.filesystem.stat(&next_root).is_ok());
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn unchanged_stamp_does_not_replace_the_live_registry() {
        let fixture = fixture_path();
        let persisted_root = fixture.join("persisted");
        let transient_root = fixture.join("transient");
        fs::create_dir_all(&persisted_root).unwrap();
        fs::create_dir_all(&transient_root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "persisted", &persisted_root);
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(settings_path),
        );

        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();
        state.filesystem.register_root(&transient_root).unwrap();
        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();

        assert!(state.filesystem.stat(&transient_root).is_ok());
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn unchanged_settings_retry_a_root_that_appears_after_a_transient_failure() {
        let fixture = fixture_path();
        fs::create_dir_all(&fixture).unwrap();
        let delayed_root = fixture.join("delayed");
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "delayed", &delayed_root);
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(settings_path),
        );

        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();
        assert!(matches!(
            state.filesystem.stat(&delayed_root),
            Err(FsError::PathOutsideScope { .. })
        ));

        fs::create_dir(&delayed_root).unwrap();
        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();

        assert!(state.filesystem.stat(&delayed_root).is_ok());
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn malformed_changed_settings_fail_before_a_filesystem_operation() {
        let fixture = fixture_path();
        let root = fixture.join("root");
        fs::create_dir_all(&root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "root", &root);
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(settings_path.clone()),
        );
        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();
        fs::write(&settings_path, b"{malformed-settings-payload").unwrap();

        let error = state
            .refresh_roots_if_changed(FS_STAT_OPERATION)
            .unwrap_err();

        assert_eq!(error.code, "settings_parse_failed");
        assert_eq!(error.context.operation, FS_STAT_OPERATION);
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn deleting_settings_revokes_every_registered_root() {
        let fixture = fixture_path();
        let root = fixture.join("root");
        fs::create_dir_all(&root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "root", &root);
        let state = NativeFilesystemState::new(
            ScopedFilesystem::new(),
            SettingsService::new(settings_path.clone()),
        );
        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();
        fs::remove_file(&settings_path).unwrap();

        state.refresh_roots_if_changed(FS_STAT_OPERATION).unwrap();

        assert!(matches!(
            state.filesystem.stat(&root),
            Err(FsError::PathOutsideScope { .. })
        ));
        drop(state);
        fs::remove_dir_all(fixture).unwrap();
    }
}
