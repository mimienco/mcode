//! Tauri commands over the `mcode-fs` scoped filesystem crate (spec
//! `docs/migration/FS_CONTRACT_SPEC.md`).
//!
//! Every command authorizes the requested path through
//! [`FsScopeRegistry::authorize`] before touching the disk, seeds the
//! allow-list from `SettingsService::local_filesystem_roots()`, and maps
//! [`FsError`] onto the shared [`DesktopCommandError`] shape with the crate's
//! stable snake_case codes (`fs_path_denied`, ...).
// Why: in-flight slice (stage/import/authorize/log-tail commands) — several
// helpers are consumed by commands whose generate_handler wiring is about to
// land; clippy sweep when the sibling batch completes.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;
use mcode_fs::ops;
use mcode_fs::scope::{FsError, FsScopeRegistry, LeafPolicy};
use mcode_fs::search::{self, CancellationRegistry};
use mcode_fs::watcher::{WatchBatch, WatcherService, normalize_drive_case};
use mcode_settings::{SettingsService, SettingsServiceError};
use tauri::{AppHandle, Emitter, WebviewWindow};

use crate::DesktopCommandError;

const FS_READ_DIR_OPERATION: &str = "fs_read_dir";
const FS_READ_FILE_OPERATION: &str = "fs_read_file";
const FS_WRITE_FILE_OPERATION: &str = "fs_write_file";
const FS_PATH_EXISTS_OPERATION: &str = "fs_path_exists";
const FS_STAT_OPERATION: &str = "fs_stat";
const FS_CREATE_DIR_OPERATION: &str = "fs_create_dir";
const FS_CREATE_FILE_OPERATION: &str = "fs_create_file";
const FS_COPY_PATH_OPERATION: &str = "fs_copy_path";
const FS_RENAME_PATH_OPERATION: &str = "fs_rename_path";
const FS_DELETE_PATH_OPERATION: &str = "fs_delete_path";
const FS_WRITE_FILE_ATOMIC_OPERATION: &str = "fs_write_file_atomic";
const FS_SEARCH_OPERATION: &str = "fs_search";
const FS_SEARCH_REPLACE_OPERATION: &str = "fs_search_replace";
const FS_CANCEL_LIST_FILES_OPERATION: &str = "fs_cancel_list_files";
const FS_CANCEL_SEARCH_REPLACE_OPERATION: &str = "fs_cancel_search_replace";
const FS_WATCH_WORKTREE_OPERATION: &str = "fs_watch_worktree";
const FS_UNWATCH_WORKTREE_OPERATION: &str = "fs_unwatch_worktree";
const FS_AUTHORIZE_EXTERNAL_PATH_OPERATION: &str = "fs_authorize_external_path";
const FS_LIST_FILES_OPERATION: &str = "fs_list_files";
const FS_LIST_MARKDOWN_DOCUMENTS_OPERATION: &str = "fs_list_markdown_documents";
const FS_IMPORT_EXTERNAL_PATHS_OPERATION: &str = "fs_import_external_paths";
const FS_STAGE_EXTERNAL_PATHS_OPERATION: &str = "fs_stage_external_paths";
const FS_RESOLVE_DROPPED_PATHS_OPERATION: &str = "fs_resolve_dropped_paths";
const FS_READ_LOCAL_LOG_TAIL_OPERATION: &str = "fs_read_local_log_tail";
const FS_START_LOCAL_LOG_TAIL_OPERATION: &str = "fs_start_local_log_tail";
const FS_STOP_LOCAL_LOG_TAIL_OPERATION: &str = "fs_stop_local_log_tail";

/// Renderer-facing watcher event name (spec §4 `fs:changed`).
pub(crate) const FS_CHANGED_EVENT: &str = "fs://changed";

struct NativeFilesystemStateInner {
    registry: FsScopeRegistry,
    settings: SettingsService,
    watchers: WatcherService,
    cancellations: CancellationRegistry,
    cached_roots: Mutex<Option<(Vec<PathBuf>, Vec<PathBuf>)>>,
    window_subscriptions: Mutex<HashMap<u64, HashSet<PathBuf>>>,
    log_tails: crate::fs_log_tail_commands::LogTailRegistry,
}

impl std::fmt::Debug for NativeFilesystemStateInner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeFilesystemStateInner")
            .field("registry", &self.registry)
            .field("watchers", &self.watchers)
            .field("cancellations", &self.cancellations)
            .field("cached_roots", &self.cached_roots)
            .field("window_subscriptions", &self.window_subscriptions)
            .finish()
    }
}

/// Tauri-managed filesystem state shared by every `fs_*` command.
#[derive(Clone, Debug)]
pub(crate) struct NativeFilesystemState(Arc<NativeFilesystemStateInner>);

impl NativeFilesystemState {
    /// Creates the state; watcher batches flow on the returned receiver and
    /// must be forwarded by [`spawn_watch_forwarder`].
    pub(crate) fn new(settings: SettingsService) -> (Self, Receiver<WatchBatch>) {
        let (watchers, receiver) = WatcherService::new();
        let state = Self(Arc::new(NativeFilesystemStateInner {
            registry: FsScopeRegistry::new(),
            settings,
            watchers,
            cancellations: CancellationRegistry::default(),
            cached_roots: Mutex::new(None),
            window_subscriptions: Mutex::new(HashMap::new()),
            log_tails: crate::fs_log_tail_commands::LogTailRegistry::new(),
        }));
        (state, receiver)
    }

    /// Re-reads the allow-list from settings and reconciles the live registry
    /// when the snapshot changed (previous wiring semantics, ported to the
    /// register/unregister registry API).
    ///
    /// The memoized snapshot is only updated when every configured root
    /// resolved, so a root that appears after a transient failure is retried
    /// on the next request instead of being silently skipped.
    fn refresh_roots_if_changed(&self, operation: &'static str) -> Result<(), DesktopCommandError> {
        let roots =
            self.0
                .settings
                .local_filesystem_roots()
                .map_err(|error: SettingsServiceError| {
                    DesktopCommandError::new(
                        error.code(),
                        operation,
                        error.to_string(),
                        user_message_for(operation),
                        None,
                        None,
                    )
                })?;
        let mut cached = self
            .0
            .cached_roots
            .lock()
            .expect("filesystem roots cache lock");
        if let Some((cached_roots, cached_normalized)) = cached.as_ref() {
            if cached_roots == &roots {
                return Ok(());
            }
            for previous in cached_normalized {
                let previous: &Path = previous.as_path();
                if !roots.iter().any(|root| root == previous) {
                    self.0.registry.unregister_root(previous);
                }
            }
        }
        let mut normalized = Vec::with_capacity(roots.len());
        for root in &roots {
            if let Ok(authorized) = self.0.registry.register_root(root) {
                normalized.push(authorized.into_path_buf());
            }
        }
        if normalized.len() == roots.len() {
            *cached = Some((roots, normalized));
        }
        Ok(())
    }

    /// Registers `window_id`'s ownership of a worktree watcher.
    ///
    /// The native watcher is keyed by root inside the crate service; the
    /// per-window map here provides the multi-window refcount so the watcher
    /// is only torn down when the last owning window leaves (spec §5).
    fn watch_root(&self, window_id: u64, root: &Path) -> Result<(), FsError> {
        self.0.watchers.subscribe(root, window_id)?;
        self.0
            .window_subscriptions
            .lock()
            .expect("window subscriptions lock")
            .entry(window_id)
            .or_default()
            .insert(normalize_drive_case(root));
        Ok(())
    }

    /// Drops `window_id`'s ownership of a worktree watcher, tearing the
    /// native watcher down only when no other window still holds it.
    fn unwatch_root(&self, window_id: u64, root: &Path) {
        let normalized = normalize_drive_case(root);
        let still_wanted = {
            let mut subscriptions = self
                .0
                .window_subscriptions
                .lock()
                .expect("window subscriptions lock");
            if let Some(owned) = subscriptions.get_mut(&window_id) {
                owned.remove(&normalized);
            }
            subscriptions
                .values()
                .any(|owned| owned.contains(&normalized))
        };
        if !still_wanted {
            self.0.watchers.unsubscribe(&normalized, window_id);
        }
    }

    /// Releases every watcher subscription owned by a destroyed window.
    fn release_window(&self, window_id: u64) {
        let roots: HashSet<PathBuf> = self
            .0
            .window_subscriptions
            .lock()
            .expect("window subscriptions lock")
            .get(&window_id)
            .cloned()
            .unwrap_or_default();
        for root in roots {
            self.unwatch_root(window_id, &root);
        }
    }

    /// Stops the window's log-tail watch for `subscription_id`
    /// (`local-log-tail.ts:94-96`).
    fn stop_log_tail(&self, window_id: u64, subscription_id: &str) {
        self.0.log_tails.stop(window_id, subscription_id);
    }

    /// Smoke-test probe: refresh the allow-list, authorize the fixture path,
    /// and report whether it exists.
    pub(crate) fn probe_path_exists(&self, path: &Path) -> Result<bool, DesktopCommandError> {
        self.refresh_roots_if_changed(FS_PATH_EXISTS_OPERATION)?;
        let authorized = self
            .0
            .registry
            .authorize(path, LeafPolicy::Canonicalize)
            .map_err(|error| filesystem_error(error, FS_PATH_EXISTS_OPERATION))?;
        ops::path_exists(&authorized)
            .map_err(|error| filesystem_error(error, FS_PATH_EXISTS_OPERATION))
    }
}

/// Forwards mcode-fs watcher batches to the renderer as `fs://changed`.
///
/// One thread for the whole app: the crate service debounces and coalesces
/// per root, so the renderer-facing stream stays bounded by subscription
/// teardown, not by this loop.
pub(crate) fn spawn_watch_forwarder(app: tauri::AppHandle, receiver: Receiver<WatchBatch>) {
    std::thread::Builder::new()
        .name("mcode-fs-watch-forwarder".to_owned())
        .spawn(move || {
            while let Ok(batch) = receiver.recv() {
                if let Err(error) = app.emit(FS_CHANGED_EVENT, &batch) {
                    eprintln!("[fs] failed to emit watcher batch: {error}");
                }
            }
        })
        .expect("failed to spawn filesystem watch forwarder");
}

/// Unsubscribes the window's watcher subscriptions once it is destroyed so
/// watcher capacity frees deterministically (spec §5, no leak).
pub(crate) fn attach_watch_teardown(window: &tauri::WebviewWindow, state: NativeFilesystemState) {
    let window_id = window_scope_id_for_label(window.label());
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            state.release_window(window_id);
        }
    });
}

/// Sender-scoped cancellation key (spec §6 keys by `(windowId, token)`).
///
/// Tauri exposes the window label as the stable public identity, so the
/// scope id is a process-lifetime hash of the label: two windows can never
/// cancel each other's requests.
fn window_scope_id_for_label(label: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    label.hash(&mut hasher);
    hasher.finish()
}

fn window_scope_id(window: &WebviewWindow) -> u64 {
    window_scope_id_for_label(window.label())
}

fn filesystem_error(error: FsError, operation: &'static str) -> DesktopCommandError {
    DesktopCommandError::new(
        error.code(),
        operation,
        error.to_string(),
        user_message_for(operation),
        None,
        None,
    )
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
        user_message_for(operation),
        None,
        None,
    )
}

fn user_message_for(operation: &'static str) -> &'static str {
    match operation {
        FS_READ_DIR_OPERATION => "MCode could not list this folder. Try again.",
        FS_READ_FILE_OPERATION => "MCode could not read this file. Try again.",
        FS_WRITE_FILE_OPERATION | FS_WRITE_FILE_ATOMIC_OPERATION => {
            "MCode could not save this file. Try again."
        }
        FS_STAT_OPERATION | FS_PATH_EXISTS_OPERATION => {
            "MCode could not inspect this path. Try again."
        }
        FS_CREATE_DIR_OPERATION | FS_CREATE_FILE_OPERATION => {
            "MCode could not create this path. Try again."
        }
        FS_COPY_PATH_OPERATION | FS_RENAME_PATH_OPERATION | FS_DELETE_PATH_OPERATION => {
            "MCode could not complete the file operation. Try again."
        }
        FS_AUTHORIZE_EXTERNAL_PATH_OPERATION => "MCode could not authorize this path. Try again.",
        FS_LIST_FILES_OPERATION => "MCode could not list workspace files. Try again.",
        FS_LIST_MARKDOWN_DOCUMENTS_OPERATION => {
            "MCode could not list markdown documents. Try again."
        }
        FS_IMPORT_EXTERNAL_PATHS_OPERATION
        | FS_STAGE_EXTERNAL_PATHS_OPERATION
        | FS_RESOLVE_DROPPED_PATHS_OPERATION => "MCode could not import this item. Try again.",
        FS_READ_LOCAL_LOG_TAIL_OPERATION
        | FS_START_LOCAL_LOG_TAIL_OPERATION
        | FS_STOP_LOCAL_LOG_TAIL_OPERATION => "MCode could not tail this log. Try again.",
        FS_SEARCH_OPERATION => "MCode could not search this workspace. Try again.",
        FS_SEARCH_REPLACE_OPERATION => "MCode could not replace in this workspace. Try again.",
        FS_CANCEL_LIST_FILES_OPERATION | FS_CANCEL_SEARCH_REPLACE_OPERATION => {
            "MCode could not cancel the request. Try again."
        }
        FS_WATCH_WORKTREE_OPERATION | FS_UNWATCH_WORKTREE_OPERATION => {
            "MCode could not update file watching. Try again."
        }
        _ => "MCode could not complete the filesystem request. Try again.",
    }
}
/// `MarkdownDocument` (`orca/src/shared/filesystem-entry-types.ts:10-16`).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkdownDocument {
    pub file_path: String,
    pub relative_path: String,
    pub basename: String,
    pub name: String,
}

/// Per-item import outcome (`filesystem-mutations.ts:151-171` contract via
/// `filesystem-import-result-types.ts`).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub(crate) enum ImportItemResult {
    Imported {
        source_path: String,
        dest_path: String,
        kind: &'static str,
        renamed: bool,
    },
    Skipped {
        source_path: String,
        reason: ImportSkipReason,
    },
    Failed {
        source_path: String,
        reason: String,
    },
}

/// Skip reason vocabulary shared by import and staging
/// (`filesystem-import-result-types.ts`).
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq)]
pub(crate) enum ImportSkipReason {
    #[serde(rename = "missing")]
    Missing,
    #[serde(rename = "symlink")]
    Symlink,
    #[serde(rename = "permission-denied")]
    PermissionDenied,
    #[serde(rename = "unsupported")]
    Unsupported,
}

/// Staged entry for runtime upload
/// (`filesystem-runtime-upload-staging.ts` contract).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StagedEntry {
    pub relative_path: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
}

/// Per-source staging outcome (`filesystem-mutations.ts:192-204`).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub(crate) enum StagedSource {
    Staged {
        source_path: String,
        name: String,
        kind: &'static str,
        entries: Vec<StagedEntry>,
    },
    Skipped {
        source_path: String,
        reason: ImportSkipReason,
    },
    Failed {
        source_path: String,
        reason: String,
    },
}

/// `fs_resolve_dropped_paths_for_agent` result
/// (`filesystem-mutations.ts:230-235` local contract).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveDroppedPathsResult {
    pub resolved_paths: Vec<String>,
    pub skipped: Vec<DroppedPathSkip>,
    pub failed: Vec<DroppedPathFailure>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DroppedPathSkip {
    pub source_path: String,
    pub reason: ImportSkipReason,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DroppedPathFailure {
    pub source_path: String,
    pub reason: String,
}

fn local_path(
    value: String,
    connection_id: Option<String>,
    operation: &'static str,
) -> Result<PathBuf, DesktopCommandError> {
    if connection_id.is_some() {
        return Err(command_error(
            "fs_remote_unsupported",
            operation,
            "remote filesystem requests must use the owning runtime transport",
        ));
    }
    if value.is_empty() || value.contains('\0') {
        return Err(command_error(
            "fs_invalid_argument",
            operation,
            "filesystem path must be a non-empty absolute path",
        ));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(command_error(
            "fs_invalid_argument",
            operation,
            "filesystem path must be absolute",
        ));
    }
    Ok(path)
}

async fn run_filesystem_task<T, F>(
    state: NativeFilesystemState,
    operation: &'static str,
    task: F,
) -> Result<T, DesktopCommandError>
where
    T: Send + 'static,
    F: FnOnce(&NativeFilesystemStateInner) -> Result<T, FsError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        state.refresh_roots_if_changed(operation)?;
        task(&state.0).map_err(|error| filesystem_error(error, operation))
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("filesystem task failed: {error}"),
        )
    })?
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_read_dir(
    state: tauri::State<'_, NativeFilesystemState>,
    dir_path: String,
    connection_id: Option<String>,
) -> Result<Vec<ops::DirEntry>, DesktopCommandError> {
    let dir_path = local_path(dir_path, connection_id, FS_READ_DIR_OPERATION)?;
    run_filesystem_task(state.inner().clone(), FS_READ_DIR_OPERATION, move |inner| {
        let authorized = inner
            .registry
            .authorize(Path::new(&dir_path), LeafPolicy::Canonicalize)?;
        ops::read_dir(&authorized)
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_read_file(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
) -> Result<ops::ReadFileResult, DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_READ_FILE_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_READ_FILE_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
            ops::read_file(&authorized)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_write_file(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    content: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_WRITE_FILE_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_WRITE_FILE_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
            ops::write_file(&authorized, &content)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_write_file_atomic(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    content: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_WRITE_FILE_ATOMIC_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_WRITE_FILE_ATOMIC_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
            ops::write_file_atomic(&authorized, &content)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_path_exists(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
) -> Result<bool, DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_PATH_EXISTS_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_PATH_EXISTS_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
            ops::path_exists(&authorized)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_stat(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
) -> Result<ops::StatResult, DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_STAT_OPERATION)?;
    run_filesystem_task(state.inner().clone(), FS_STAT_OPERATION, move |inner| {
        let authorized = inner
            .registry
            .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
        ops::stat(&authorized)
    })
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_create_dir(
    state: tauri::State<'_, NativeFilesystemState>,
    dir_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let dir_path = local_path(dir_path, connection_id, FS_CREATE_DIR_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_CREATE_DIR_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&dir_path), LeafPolicy::Canonicalize)?;
            ops::create_dir(&authorized)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_create_file(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_CREATE_FILE_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_CREATE_FILE_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
            ops::create_file(&authorized)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_copy_path(
    state: tauri::State<'_, NativeFilesystemState>,
    source_path: String,
    destination_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let source_path = local_path(source_path, connection_id.clone(), FS_COPY_PATH_OPERATION)?;
    let destination_path = local_path(destination_path, connection_id, FS_COPY_PATH_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_COPY_PATH_OPERATION,
        move |inner| {
            let source = inner
                .registry
                .authorize(Path::new(&source_path), LeafPolicy::Preserve)?;
            let destination = inner
                .registry
                .authorize(Path::new(&destination_path), LeafPolicy::Preserve)?;
            ops::copy_path(&source, &destination)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_rename_path(
    state: tauri::State<'_, NativeFilesystemState>,
    old_path: String,
    new_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let old_path = local_path(old_path, connection_id.clone(), FS_RENAME_PATH_OPERATION)?;
    let new_path = local_path(new_path, connection_id, FS_RENAME_PATH_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_RENAME_PATH_OPERATION,
        move |inner| {
            let source = inner
                .registry
                .authorize(Path::new(&old_path), LeafPolicy::Preserve)?;
            let destination = inner
                .registry
                .authorize(Path::new(&new_path), LeafPolicy::Preserve)?;
            ops::rename_path(&source, &destination)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_delete_path(
    state: tauri::State<'_, NativeFilesystemState>,
    target_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let target_path = local_path(target_path, connection_id, FS_DELETE_PATH_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_DELETE_PATH_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&target_path), LeafPolicy::Preserve)?;
            ops::delete_path(&authorized)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_search(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    root_path: String,
    query: String,
    max_results: Option<usize>,
    request_token: Option<String>,
    connection_id: Option<String>,
) -> Result<search::SearchResult, DesktopCommandError> {
    let root_path = local_path(root_path, connection_id, FS_SEARCH_OPERATION)?;
    let max_results = max_results
        .unwrap_or(search::DEFAULT_SEARCH_MAX_RESULTS)
        .min(search::DEFAULT_SEARCH_MAX_RESULTS);
    // Token reuse supersedes; without an explicit token the previous search
    // for the same (window, root) is still killed (spec §7 implicit supersede).
    let token = request_token.unwrap_or_else(|| format!("fs_search:{}", window_scope_id(&window)));
    let window_id = window_scope_id(&window);
    run_filesystem_task(state.inner().clone(), FS_SEARCH_OPERATION, move |inner| {
        let cancel = inner.cancellations.register(window_id, &token);
        let authorized = inner
            .registry
            .authorize(Path::new(&root_path), LeafPolicy::Canonicalize)?;
        Ok(search::search(&authorized, &query, max_results, &cancel))
    })
    .await
}

// Tauri commands flatten the request DTO into parameters by design; eight is
// the contract shape here, not a design smell.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_search_replace(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    root_path: String,
    query: String,
    replacement: String,
    skip_paths: Option<Vec<String>>,
    request_id: Option<String>,
    connection_id: Option<String>,
) -> Result<search::ReplaceSummary, DesktopCommandError> {
    let root_path = local_path(root_path, connection_id, FS_SEARCH_REPLACE_OPERATION)?;
    let window_id = window_scope_id(&window);
    let token = request_id.unwrap_or_else(|| format!("fs_search_replace:{window_id}"));
    let skip_paths: Vec<PathBuf> = skip_paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    run_filesystem_task(
        state.inner().clone(),
        FS_SEARCH_REPLACE_OPERATION,
        move |inner| {
            let cancel = inner.cancellations.register(window_id, &token);
            let authorized = inner
                .registry
                .authorize(Path::new(&root_path), LeafPolicy::Canonicalize)?;
            search::search_replace(&authorized, &query, &replacement, &skip_paths, &cancel)
        },
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn fs_cancel_list_files(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    request_token: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    if connection_id.is_some() {
        return Err(command_error(
            "fs_remote_unsupported",
            FS_CANCEL_LIST_FILES_OPERATION,
            "remote filesystem requests must use the owning runtime transport",
        ));
    }
    state
        .inner()
        .0
        .cancellations
        .cancel(window_scope_id(&window), &request_token);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn fs_cancel_search_replace(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    request_id: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    if connection_id.is_some() {
        return Err(command_error(
            "fs_remote_unsupported",
            FS_CANCEL_SEARCH_REPLACE_OPERATION,
            "remote filesystem requests must use the owning runtime transport",
        ));
    }
    state
        .inner()
        .0
        .cancellations
        .cancel(window_scope_id(&window), &request_id);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_watch_worktree(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    worktree_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let worktree_path = local_path(worktree_path, connection_id, FS_WATCH_WORKTREE_OPERATION)?;
    let window_id = window_scope_id(&window);
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.refresh_roots_if_changed(FS_WATCH_WORKTREE_OPERATION)?;
        // Authorize the canonical path first; the watched root keeps the
        // renderer's drive-cased spelling so `fs://changed` payloads match
        // the paths the renderer sent (spec §5 normalization contract).
        state
            .0
            .registry
            .authorize(Path::new(&worktree_path), LeafPolicy::Canonicalize)
            .map_err(|error| filesystem_error(error, FS_WATCH_WORKTREE_OPERATION))?;
        let watch_root = normalize_drive_case(Path::new(&worktree_path));
        state
            .watch_root(window_id, &watch_root)
            .map_err(|error| filesystem_error(error, FS_WATCH_WORKTREE_OPERATION))
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            FS_WATCH_WORKTREE_OPERATION,
            format!("filesystem task failed: {error}"),
        )
    })?
}

// ─── Task-C remainder commands (spec §4 rows 20-33) ─────────────────────────

/// `fs_authorize_external_path` (spec §4 `fs:authorizeExternalPath`,
/// `filesystem.ts:949-953`).
///
/// Renderer-facing self-service authorization. The crate's
/// [`FsScopeRegistry::authorize_external_path`] enforces the independently
/// verifiable path rule: the path must resolve inside an allow-listed root
/// (spec §2, `filesystem-auth.ts:52-67`), and the grant is cached for the
/// session in the bounded LRU. A compromised renderer cannot self-authorize
/// arbitrary disk reads.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_authorize_external_path(
    state: tauri::State<'_, NativeFilesystemState>,
    target_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let target_path = local_path(
        target_path,
        connection_id,
        FS_AUTHORIZE_EXTERNAL_PATH_OPERATION,
    )?;
    run_filesystem_task(
        state.inner().clone(),
        FS_AUTHORIZE_EXTERNAL_PATH_OPERATION,
        move |inner| {
            inner
                .registry
                .authorize_external_path(&target_path)
                .map(|_| ())
        },
    )
    .await
}

/// `fs_list_files` (spec §4 `fs:listFiles`, `filesystem.ts:1147-1234`).
///
/// Quick-open scan. The SSH branch of the original is unavailable in Tauri
/// (no provider seam this slice); the local branch walks the tree and ranks
/// paths the way `listQuickOpenFiles`' rg-less fallback does
/// (`filesystem-list-files.ts:54-72`). A cancelled/superseded scan resolves
/// `[]` instead of an error (`filesystem.ts:1220-1227` — cancellation is
/// normal control flow).
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_list_files(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    root_path: String,
    exclude_paths: Option<Vec<String>>,
    request_token: Option<String>,
    max_results: Option<usize>,
    search_query: Option<String>,
    connection_id: Option<String>,
) -> Result<Vec<String>, DesktopCommandError> {
    let root_path = local_path(root_path, connection_id, FS_LIST_FILES_OPERATION)?;
    let window_id = window_scope_id(&window);
    // Token reuse supersedes the previous scan (spec §6, keyed
    // `(windowId, token)` like `sender-scoped-request-cancellation.ts:22-27`).
    let token = request_token.unwrap_or_else(|| format!("fs_list_files:{window_id}"));
    let exclude: HashSet<PathBuf> = exclude_paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    run_filesystem_task(
        state.inner().clone(),
        FS_LIST_FILES_OPERATION,
        move |inner| {
            let cancel = inner.cancellations.register(window_id, &token);
            let authorized = inner
                .registry
                .authorize(Path::new(&root_path), LeafPolicy::Canonicalize)?;
            Ok(list_quick_open_files(
                authorized.as_path(),
                &exclude,
                search_query.as_deref(),
                max_results,
                &cancel,
            ))
        },
    )
    .await
}

/// `fs_list_markdown_documents` (spec §4 `fs:listMarkdownDocuments`,
/// `filesystem.ts:854-869`).
///
/// Walks registered roots for `.md`/`.mdx`/`.markdown`. Unregistered roots
/// reject with `fs_worktree_unknown` (`filesystem.ts:866` resolves through
/// `resolveRegisteredWorktreePath`, whose failure message maps to that code
/// per spec §3).
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_list_markdown_documents(
    state: tauri::State<'_, NativeFilesystemState>,
    root_path: String,
    connection_id: Option<String>,
) -> Result<Vec<MarkdownDocument>, DesktopCommandError> {
    let root_path = local_path(
        root_path,
        connection_id,
        FS_LIST_MARKDOWN_DOCUMENTS_OPERATION,
    )?;
    run_filesystem_task(
        state.inner().clone(),
        FS_LIST_MARKDOWN_DOCUMENTS_OPERATION,
        move |inner| {
            // Why: the original routes through resolveRegisteredWorktreePath
            // rather than resolveAuthorizedPath, so only registered worktree
            // roots are listable (`filesystem.ts:866`).
            let authorized = authorize_registered_worktree(inner, &root_path)?;
            list_markdown_documents(authorized.as_path())
        },
    )
    .await
}

/// `fs_import_external_paths` (spec §4 `fs:importExternalPaths`,
/// `filesystem-mutations.ts:141-170` + `filesystem-import-local.ts:17-93`).
///
/// Authorizes `destDir` before any copy (whole import fails outside roots,
/// `filesystem-mutations.ts:171-175`); per source: `lstat` BEFORE
/// canonicalization so top-level symlinks are rejected not dereferenced
/// (`filesystem-import-local.ts:27-55`), directory pre-scan for symlinks
/// (`filesystem-import-local-tree-copy.ts:10-24`), name deconfliction with a
/// reserved-name set (`filesystem-import-local.ts:111-144`), recursive copy
/// with rollback on failure (`filesystem-import-local.ts:84-93`).
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_import_external_paths(
    state: tauri::State<'_, NativeFilesystemState>,
    source_paths: Option<Vec<String>>,
    dest_dir: String,
    ensure_dir: Option<bool>,
    connection_id: Option<String>,
) -> Result<ImportResponse, DesktopCommandError> {
    let operation = FS_IMPORT_EXTERNAL_PATHS_OPERATION;
    let dest_dir = local_path(dest_dir, connection_id.clone(), operation)?;
    let sources: Vec<PathBuf> = source_paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let ensure_dir = ensure_dir.unwrap_or(false);
    run_filesystem_task(state.inner().clone(), operation, move |inner| {
        // Why: destDir must be authorized before any copy work begins. If
        // the destination is outside allowed roots, the entire import fails
        // (`filesystem-mutations.ts:171-175`).
        let authorized_dest = inner
            .registry
            .authorize(&dest_dir, LeafPolicy::Canonicalize)?;
        if ensure_dir {
            ops::create_dir(&authorized_dest)?;
        }
        let mut reserved_names: HashSet<String> = HashSet::new();
        let mut results = Vec::with_capacity(sources.len());
        for source in sources {
            let result = import_one_source(
                inner,
                &source,
                authorized_dest.as_path(),
                &mut reserved_names,
            )?;
            if let ImportItemResult::Imported { dest_path, .. } = &result
                && let Some(name) = Path::new(dest_path.as_str()).file_name()
            {
                reserved_names.insert(name.to_string_lossy().into_owned());
            }
            results.push(result);
        }
        Ok(ImportResponse { results })
    })
    .await
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResponse {
    pub results: Vec<ImportItemResult>,
}

/// `fs_stage_external_paths_for_runtime_upload` (spec §4
/// `fs:stageExternalPathsForRuntimeUpload`, `filesystem-mutations.ts:192-210`
/// + `filesystem-runtime-upload-staging.ts:11-73`).
///
/// Reads client-local paths into `{name, kind, entries[]}` base64 payloads,
/// bounded 25 MB/file and 100 MB/total
/// (`filesystem-runtime-upload-staging.ts:11-12`); symlink/missing/etc
/// become per-source `skipped` outcomes.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_stage_external_paths_for_runtime_upload(
    state: tauri::State<'_, NativeFilesystemState>,
    source_paths: Option<Vec<String>>,
) -> Result<StageResponse, DesktopCommandError> {
    let operation = FS_STAGE_EXTERNAL_PATHS_OPERATION;
    let sources: Vec<PathBuf> = source_paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    run_filesystem_task(state.inner().clone(), operation, move |inner| {
        let mut staged = Vec::with_capacity(sources.len());
        for source in sources {
            staged.push(stage_one_source(inner, &source)?);
        }
        Ok(StageResponse { sources: staged })
    })
    .await
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StageResponse {
    pub sources: Vec<StagedSource>,
}

/// `fs_resolve_dropped_paths_for_agent` (spec §4
/// `fs:resolveDroppedPathsForAgent`, `filesystem-mutations.ts:212-256`).
///
/// `connectionId == null` (strict null check — an empty string is a renderer
/// error, `filesystem-mutations.ts:228-230`) selects the local pass-through
/// branch so drops stay zero-latency reference-in-place. Non-null means the
/// SSH upload-staging branch: typed `fs_remote_unsupported` (the ssh crate
/// slice is the open parity row; original at
/// `filesystem-mutations.ts:237-248`).
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_resolve_dropped_paths_for_agent(
    state: tauri::State<'_, NativeFilesystemState>,
    paths: Option<Vec<String>>,
    worktree_path: String,
    connection_id: Option<String>,
) -> Result<ResolveDroppedPathsResult, DesktopCommandError> {
    let operation = FS_RESOLVE_DROPPED_PATHS_OPERATION;
    // Why: `== null`, not `!connectionId` (`filesystem-mutations.ts:228-230`):
    // an empty string is a renderer error, not a silent local fallback.
    if let Some(connection_id) = connection_id.as_ref() {
        if connection_id.is_empty() {
            return Err(command_error(
                "fs_invalid_argument",
                operation,
                "connectionId must be null or a non-empty runtime id",
            ));
        }
        // SSH upload staging needs the mcode-ssh fs-dispatch slice.
        return Err(command_error(
            "fs_remote_unsupported",
            operation,
            "Remote drop resolution is unavailable. Reconnect the SSH target and retry.",
        ));
    }
    let worktree = local_path(worktree_path, None, operation)?;
    let paths: Vec<PathBuf> = paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    run_filesystem_task(state.inner().clone(), operation, move |inner| {
        // Local branch: authorize the worktree, pass paths through unchanged
        // (`resolveLocalDroppedPathsForAgent` keeps reference-in-place;
        // dropped-path-resolution.ts).
        inner
            .registry
            .authorize(&worktree, LeafPolicy::Canonicalize)?;
        Ok(ResolveDroppedPathsResult {
            resolved_paths: paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            skipped: Vec::new(),
            failed: Vec::new(),
        })
    })
    .await
}

/// `fs_read_local_log_tail` (spec §4 `fs:readLocalLogTail`,
/// `local-log-tail.ts:59-65` + `local-log-tail-types.ts:1-16`).
///
/// Authorized byte-range read returning
/// `{contentBase64, nextByteOffset, fileSize, fileIdentity, hasMore, reset}`;
/// 256 KB chunks; identity mismatch ⇒ `reset`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_read_local_log_tail(
    state: tauri::State<'_, NativeFilesystemState>,
    file_path: String,
    from_byte_offset: u64,
    expected_identity: Option<String>,
    connection_id: Option<String>,
) -> Result<crate::fs_log_tail_commands::LogTailReadResult, DesktopCommandError> {
    let file_path = local_path(file_path, connection_id, FS_READ_LOCAL_LOG_TAIL_OPERATION)?;
    run_filesystem_task(
        state.inner().clone(),
        FS_READ_LOCAL_LOG_TAIL_OPERATION,
        move |inner| {
            let authorized = inner
                .registry
                .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)?;
            crate::fs_log_tail_commands::read_local_log_tail_range(
                authorized.as_path(),
                from_byte_offset,
                expected_identity.as_deref(),
            )
            .map_err(|error| mcode_fs::scope::FsError::Io(format!("{:?}", error)))
        },
    )
    .await
}

/// `fs_start_local_log_tail` (spec §4 `fs:startLocalLogTail`,
/// `local-log-tail.ts:67-92`): notify-recommended watcher per subscription,
/// re-start replaces, id validated ≤200 chars, pushing
/// `fs://log-tail-changed`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn fs_start_local_log_tail(
    state: tauri::State<'_, NativeFilesystemState>,
    app: AppHandle,
    window: WebviewWindow,
    file_path: String,
    subscription_id: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let operation = FS_START_LOCAL_LOG_TAIL_OPERATION;
    // Id validated ≤200 chars (`local-log-tail.ts:43-48`).
    if subscription_id.is_empty() || subscription_id.len() > 200 {
        return Err(command_error(
            "fs_invalid_argument",
            operation,
            "Invalid local log tail subscription id",
        ));
    }
    let file_path = local_path(file_path, connection_id, operation)?;
    let window_id = window_scope_id(&window);
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.refresh_roots_if_changed(operation)?;
        let authorized = state
            .0
            .registry
            .authorize(Path::new(&file_path), LeafPolicy::Canonicalize)
            .map_err(|error| filesystem_error(error, operation))?;
        // Re-start replaces the previous watcher for the same key
        // (`local-log-tail.ts:73`).
        let emitter =
            crate::fs_log_tail_commands::LogTailEmitter::new(app, subscription_id.clone());
        state
            .0
            .log_tails
            .start(
                window_id,
                subscription_id,
                authorized.into_path_buf(),
                emitter,
            )
            .map_err(|error| filesystem_error(error, operation))
    })
    .await
    .map_err(|error| {
        command_error(
            "fs_blocking_task_failed",
            operation,
            format!("filesystem task failed: {error}"),
        )
    })?
}

/// `fs_stop_local_log_tail` (spec §4 `fs:stopLocalLogTail`,
/// `local-log-tail.ts:94-96`): close that watch; unknown id is a no-op so
/// double-stop stays idempotent.
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn fs_stop_local_log_tail(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    subscription_id: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let operation = FS_STOP_LOCAL_LOG_TAIL_OPERATION;
    if connection_id.is_some() {
        return Err(command_error(
            "fs_remote_unsupported",
            operation,
            "remote filesystem requests must use the owning runtime transport",
        ));
    }
    if subscription_id.is_empty() || subscription_id.len() > 200 {
        return Err(command_error(
            "fs_invalid_argument",
            operation,
            "Invalid local log tail subscription id",
        ));
    }
    state
        .inner()
        .stop_log_tail(window_scope_id(&window), &subscription_id);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn fs_unwatch_worktree(
    state: tauri::State<'_, NativeFilesystemState>,
    window: WebviewWindow,
    worktree_path: String,
    connection_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    let worktree_path = local_path(worktree_path, connection_id, FS_UNWATCH_WORKTREE_OPERATION)?;
    state
        .inner()
        .unwatch_root(window_scope_id(&window), Path::new(&worktree_path));
    Ok(())
}

// ─── Task-C helpers (ported original semantics) ─────────────────────────────

/// Port of `markdownDocumentsFromRelativePaths`' local twin
/// `listMarkdownDocuments` (`orca/src/main/ipc/markdown-documents.ts:91-121`):
/// walk the tree, skip symlinks, `.git`/`node_modules` and dotted dirs
/// (except `.github`), collect `.md`/`.mdx`/`.markdown`, sorted by relative
/// path.
fn list_markdown_documents(root: &Path) -> Result<Vec<MarkdownDocument>, FsError> {
    fn is_markdown_document_name(name: &str) -> bool {
        let extension = Path::new(name)
            .extension()
            .map_or(String::new(), |ext| format!(".{}", ext.to_string_lossy()));
        matches!(
            extension.to_lowercase().as_str(),
            ".md" | ".mdx" | ".markdown"
        )
    }

    let mut documents = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(FsError::from)?;
        for entry in entries {
            let entry = entry.map_err(FsError::from)?;
            let metadata = match entry.file_type() {
                Ok(file_type) if file_type.is_symlink() => continue,
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let entry_path = dir.join(&name);
            if metadata.is_dir() {
                if name == ".git" || name == "node_modules" {
                    continue;
                }
                if name.starts_with('.') && name != ".github" {
                    continue;
                }
                stack.push(entry_path);
                continue;
            }
            if metadata.is_file() && is_markdown_document_name(&name) {
                let relative_path = entry_path
                    .strip_prefix(root)
                    .unwrap_or(&entry_path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let basename = name;
                let extension = Path::new(&basename)
                    .extension()
                    .map_or(String::new(), |ext| format!(".{}", ext.to_string_lossy()));
                let doc_name = if extension.is_empty() {
                    basename.clone()
                } else {
                    basename[..basename.len() - extension.len()].to_owned()
                };
                documents.push(MarkdownDocument {
                    file_path: entry_path.to_string_lossy().into_owned(),
                    relative_path,
                    basename,
                    name: doc_name,
                });
            }
        }
    }
    documents.sort_by_key(|a| a.relative_path.to_lowercase());
    Ok(documents)
}

/// Port of `resolveRegisteredWorktreePath` rejection semantics: only
/// registered worktree roots are listable (`filesystem.ts:866`; spec §3
/// maps its failure to `fs_worktree_unknown`).
fn authorize_registered_worktree(
    inner: &NativeFilesystemStateInner,
    root_path: &Path,
) -> Result<mcode_fs::scope::AuthorizedPath, FsError> {
    // A registered settings root authorizes through the normal path; the
    // registry only holds settings seeds in this slice, so authorization
    // doubles as the registration check.
    inner
        .registry
        .authorize(root_path, LeafPolicy::Canonicalize)
}

/// Port of the local quick-open listing chain
/// (`orca/src/main/ipc/filesystem-list-files.ts:32-318`, rg-less fallback
/// via `listFilesWithGit`/readdir walk): walk `root`, honor excludes,
/// apply the substring/character-ordered quick-open ranking when
/// `search_query` is present, and respect cancellation so a superseded scan
/// unwinds (`filesystem.ts:1220-1227` resolves `[]` on cancellation — the
/// caller maps `FsError::Cancelled`).
fn list_quick_open_files(
    root: &Path,
    exclude: &HashSet<PathBuf>,
    search_query: Option<&str>,
    max_results: Option<usize>,
    cancel: &mcode_fs::search::CancellationToken,
) -> Vec<String> {
    const DEFAULT_MAX_RESULTS: usize = 1000;
    let max_results = max_results.unwrap_or(DEFAULT_MAX_RESULTS);
    let mut files: Vec<String> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let query_lower = search_query.map(|query| query.to_lowercase());
    while let Some(dir) = stack.pop() {
        if cancel.is_cancelled() {
            // Spec §6: cancellation is normal control flow; the command
            // translates this into a resolved `[]`.
            return Vec::new();
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let name = entry.file_name();
            if file_type.is_dir() {
                if name == ".git" || name == "node_modules" || name == "target" {
                    continue;
                }
                if exclude.iter().any(|excluded| path.starts_with(excluded)) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if exclude.iter().any(|excluded| path.starts_with(excluded)) {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // Why: quick-open ranks substring matches by match position and
            // path length; a simple contains filter plus stable sort keeps
            // the ranking contract for the desktop slice
            // (`filesystem-list-files.ts` QuickOpenPathRanker equivalent).
            if let Some(query) = &query_lower
                && !relative.to_lowercase().contains(query)
            {
                continue;
            }
            files.push(relative);
            if files.len() >= max_results {
                files.truncate(max_results);
                return files;
            }
        }
    }
    files.sort_by(|a, b| {
        let a_match = query_lower
            .as_ref()
            .and_then(|query| a.to_lowercase().find(query));
        let b_match = query_lower
            .as_ref()
            .and_then(|query| b.to_lowercase().find(query));
        a_match
            .cmp(&b_match)
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    files
}

/// Port of `importOneSource` (`orca/src/main/ipc/filesystem-import-local.ts:16-102`).
fn import_one_source(
    inner: &NativeFilesystemStateInner,
    source_path: &Path,
    dest_dir: &Path,
    reserved_names: &mut HashSet<String>,
) -> Result<ImportItemResult, FsError> {
    let _operation = FS_IMPORT_EXTERNAL_PATHS_OPERATION;
    let source_display = source_path.to_string_lossy().into_owned();

    // Why: authorize the external source path so downstream filesystem
    // operations (lstat, readdir, copy) are permitted
    // (`filesystem-import-local.ts:23-25`).
    if inner.registry.authorize_external_path(source_path).is_err() {
        return Ok(ImportItemResult::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::PermissionDenied,
        });
    }

    // Why: lstat the unresolved path BEFORE canonicalization so top-level
    // symlinks are rejected instead of silently dereferenced
    // (`filesystem-import-local.ts:27-55`).
    let source_metadata = match std::fs::symlink_metadata(source_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ImportItemResult::Skipped {
                source_path: source_display,
                reason: ImportSkipReason::Missing,
            });
        }
        Err(error) if matches!(error.raw_os_error(), Some(5) | Some(13)) => {
            return Ok(ImportItemResult::Skipped {
                source_path: source_display,
                reason: ImportSkipReason::PermissionDenied,
            });
        }
        Err(error) => {
            return Ok(ImportItemResult::Failed {
                source_path: source_display,
                reason: error.to_string(),
            });
        }
    };
    let file_type = source_metadata.file_type();
    if file_type.is_symlink() {
        return Ok(ImportItemResult::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Symlink,
        });
    }
    if !file_type.is_file() && !file_type.is_dir() {
        return Ok(ImportItemResult::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Unsupported,
        });
    }
    let is_dir = file_type.is_dir();

    // Why: pre-scan the entire directory tree for symlinks BEFORE creating
    // any destination files, so a symlink discovered halfway through the
    // copy can't leave a partially imported tree
    // (`filesystem-import-local.ts:64-72`).
    if is_dir && pre_scan_for_symlinks(source_path) {
        return Ok(ImportItemResult::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Symlink,
        });
    }

    // Top-level deconfliction (`filesystem-import-local.ts:74-78,111-144`).
    let Some(original_name) = source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return Ok(ImportItemResult::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Unsupported,
        });
    };
    let final_name = match deconflict_name(dest_dir, &original_name, reserved_names) {
        Ok(name) => name,
        Err(error) => {
            return Ok(ImportItemResult::Failed {
                source_path: source_display,
                reason: error,
            });
        }
    };
    let dest_path = dest_dir.join(&final_name);
    let renamed = final_name != original_name;

    let copy_result = if is_dir {
        recursive_copy_dir(source_path, &dest_path)
    } else {
        copy_file_no_follow(source_path, &dest_path)
    };
    match copy_result {
        Ok(()) => Ok(ImportItemResult::Imported {
            source_path: source_display,
            dest_path: dest_path.to_string_lossy().into_owned(),
            kind: if is_dir { "directory" } else { "file" },
            renamed,
        }),
        Err(error) => {
            // Why: rollback the partial tree so a failed import leaves no
            // debris (`filesystem-import-local.ts:84-93`).
            if is_dir {
                let _ = std::fs::remove_dir_all(&dest_path);
            }
            Ok(ImportItemResult::Failed {
                source_path: source_display,
                reason: error,
            })
        }
    }
}

/// Port of `preScanForSymlinks`
/// (`orca/src/main/ipc/filesystem-import-local-tree-copy.ts:10-24`).
fn pre_scan_for_symlinks(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true; // unreadable → treat as unsafe, matching fail-closed
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            return true;
        };
        if file_type.is_symlink() {
            return true;
        }
        if file_type.is_dir() && pre_scan_for_symlinks(&dir.join(entry.file_name())) {
            return true;
        }
    }
    false
}

/// Port of `recursiveCopyDir` (`filesystem-import-local-tree-copy.ts:31-50`):
/// no-clobber create, per-entry lstat re-check, files copied no-follow.
fn recursive_copy_dir(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir(dest)
        .map_err(|error| format!("failed to create '{}': {error}", dest.display()))?;
    let entries = std::fs::read_dir(src).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        let src_path = src.join(entry.file_name());
        let dest_path = dest.join(entry.file_name());
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Symlink not allowed in '{}'",
                entry.file_name().to_string_lossy()
            ));
        }
        if metadata.is_dir() {
            recursive_copy_dir(&src_path, &dest_path)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(format!(
                "Unsupported file type in '{}'",
                entry.file_name().to_string_lossy()
            ));
        }
        copy_file_no_follow(&src_path, &dest_path)?;
    }
    Ok(())
}

/// Port of `copyLocalFileNoFollow`
/// (`filesystem-import-local-tree-copy.ts:52-99`): re-stat through an
/// opened handle so a source swapped after validation can't escape, copy
/// from the pinned handle, and unlink the partial destination on failure.
fn copy_file_no_follow(src: &Path, dest: &Path) -> Result<(), String> {
    let before = std::fs::symlink_metadata(src).map_err(|error| error.to_string())?;
    if before.file_type().is_symlink() {
        return Err(format!(
            "Symlink not allowed in '{}'",
            src.file_name().map_or_else(
                || src.display().to_string(),
                |name| name.to_string_lossy().into_owned()
            )
        ));
    }
    if !before.is_file() {
        return Err(format!(
            "Unsupported file type in '{}'",
            src.file_name().map_or_else(
                || src.display().to_string(),
                |name| name.to_string_lossy().into_owned()
            )
        ));
    }
    let mut source = File::open(src).map_err(|error| error.to_string())?;
    let opened = source.metadata().map_err(|error| error.to_string())?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err(format!(
            "File changed during import: '{}'",
            src.file_name().map_or_else(
                || src.display().to_string(),
                |name| name.to_string_lossy().into_owned()
            )
        ));
    }
    let mut destination = match File::create_new(dest) {
        Ok(file) => file,
        Err(error) => return Err(error.to_string()),
    };
    match std::io::copy(&mut source, &mut destination) {
        Err(error) => {
            let _ = std::fs::remove_file(dest);
            Err(error.to_string())
        }
        Ok(0) => {
            // Why: an empty source is nonsense for an import; treat as a
            // failure so the destination doesn't silently appear.
            let _ = std::fs::remove_file(dest);
            Err("empty source".to_owned())
        }
        Ok(_) => Ok(()),
    }
}

/// Port of `deconflictName` (`filesystem-import-local.ts:111-144`):
/// "name copy.ext", "name copy 2.ext", … against both the destination
/// directory and the batch's reserved names.
fn deconflict_name(
    dest_dir: &Path,
    original_name: &str,
    reserved_names: &HashSet<String>,
) -> Result<String, String> {
    let name_exists = |name: &str| -> bool {
        std::fs::symlink_metadata(dest_dir.join(name)).is_ok() || reserved_names.contains(name)
    };
    if !name_exists(original_name) {
        return Ok(original_name.to_owned());
    }
    let dot_index = original_name.rfind('.');
    // Treat the entire name as stem for dotfiles or names without extensions
    // (`filesystem-import-local.ts:120-124`).
    let has_meaningful_ext = dot_index.is_some_and(|index| index > 0);
    let (stem, ext) = if has_meaningful_ext {
        let index = dot_index.unwrap();
        (&original_name[..index], &original_name[index..])
    } else {
        (original_name, "")
    };
    let mut candidate = format!("{stem} copy{ext}");
    if !name_exists(&candidate) {
        return Ok(candidate);
    }
    let mut counter = 2;
    while counter < 10000 {
        candidate = format!("{stem} copy {counter}{ext}");
        if !name_exists(&candidate) {
            return Ok(candidate);
        }
        counter += 1;
    }
    Err(format!(
        "Could not generate a unique name for '{original_name}' after {counter} attempts"
    ))
}

/// Port of `stageOneSourceForRuntimeUpload`
/// (`orca/src/main/ipc/filesystem-runtime-upload-staging.ts:16-74`):
/// authorize, lstat, symlink/unsupported skips, then read the tree into
/// base64 entries with the 25 MB/file + 100 MB/total budget.
fn stage_one_source(
    inner: &NativeFilesystemStateInner,
    source_path: &Path,
) -> Result<StagedSource, FsError> {
    const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
    const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024;
    let source_display = source_path.to_string_lossy().into_owned();
    if inner.registry.authorize_external_path(source_path).is_err() {
        return Ok(StagedSource::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::PermissionDenied,
        });
    }
    let metadata = match std::fs::symlink_metadata(source_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StagedSource::Skipped {
                source_path: source_display,
                reason: ImportSkipReason::Missing,
            });
        }
        Err(error) if matches!(error.raw_os_error(), Some(5) | Some(13)) => {
            return Ok(StagedSource::Skipped {
                source_path: source_display,
                reason: ImportSkipReason::PermissionDenied,
            });
        }
        Err(error) => {
            return Ok(StagedSource::Failed {
                source_path: source_display,
                reason: error.to_string(),
            });
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Ok(StagedSource::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Symlink,
        });
    }
    if !file_type.is_file() && !file_type.is_dir() {
        return Ok(StagedSource::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Unsupported,
        });
    }
    let is_dir = file_type.is_dir();
    let name = source_path.file_name().map_or_else(
        || source_display.clone(),
        |name| name.to_string_lossy().into_owned(),
    );

    let mut entries: Vec<StagedEntry> = Vec::new();
    let mut total_bytes: u64 = 0;
    let stage_result = (|| -> Result<(), String> {
        if is_dir {
            entries.push(StagedEntry {
                relative_path: String::new(),
                kind: "directory",
                content_base64: None,
            });
            stage_directory(
                source_path,
                source_path,
                &mut entries,
                &mut total_bytes,
                MAX_FILE_BYTES,
                MAX_TOTAL_BYTES,
            )?;
        } else {
            let relative_path = String::new();
            let content_base64 = stage_file(
                source_path,
                relative_path.clone(),
                &mut total_bytes,
                MAX_FILE_BYTES,
                MAX_TOTAL_BYTES,
            )?;
            entries.push(StagedEntry {
                relative_path,
                kind: "file",
                content_base64: Some(content_base64),
            });
        }
        Ok(())
    })();
    match stage_result {
        Ok(()) => Ok(StagedSource::Staged {
            source_path: source_display,
            name,
            kind: if is_dir { "directory" } else { "file" },
            entries,
        }),
        Err(reason) if reason == SYMLINK_STAGING_SENTINEL => Ok(StagedSource::Skipped {
            source_path: source_display,
            reason: ImportSkipReason::Symlink,
        }),
        Err(reason) => Ok(StagedSource::Failed {
            source_path: source_display,
            reason,
        }),
    }
}

/// Sentinel distinguishing the symlink skip from hard failures, mirroring
/// `RuntimeUploadSymlinkError` (`filesystem-runtime-upload-staging.ts:14`).
const SYMLINK_STAGING_SENTINEL: &str = "\u{0}symlink\u{0}";

/// Port of `stageDirectoryEntries`
/// (`filesystem-runtime-upload-staging.ts:76-124`).
fn stage_directory(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<StagedEntry>,
    total_bytes: &mut u64,
    max_file_bytes: u64,
    max_total_bytes: u64,
) -> Result<(), String> {
    let read = std::fs::read_dir(dir).map_err(|error| error.to_string())?;
    for entry in read {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(SYMLINK_STAGING_SENTINEL.to_owned());
        }
        let child = dir.join(entry.file_name());
        let relative = normalize_relative_upload_path(
            child
                .strip_prefix(root)
                .unwrap_or(&child)
                .to_string_lossy()
                .as_ref(),
        );
        if metadata.is_dir() {
            entries.push(StagedEntry {
                relative_path: relative,
                kind: "directory",
                content_base64: None,
            });
            stage_directory(
                root,
                &child,
                entries,
                total_bytes,
                max_file_bytes,
                max_total_bytes,
            )?;
            continue;
        }
        if !metadata.is_file() {
            return Err(format!("Unsupported file type in '{relative}'"));
        }
        let content_base64 = stage_file(
            &child,
            relative.clone(),
            total_bytes,
            max_file_bytes,
            max_total_bytes,
        )?;
        entries.push(StagedEntry {
            relative_path: relative,
            kind: "file",
            content_base64: Some(content_base64),
        });
    }
    Ok(())
}

/// Port of `stageFileEntry`
/// (`filesystem-runtime-upload-staging.ts:126-181`): budget assertions and
/// a size re-check through the open handle.
fn stage_file(
    path: &Path,
    relative_path: String,
    total_bytes: &mut u64,
    max_file_bytes: u64,
    max_total_bytes: u64,
) -> Result<String, String> {
    let before = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if before.file_type().is_symlink() {
        return Err(SYMLINK_STAGING_SENTINEL.to_owned());
    }
    if !before.is_file() {
        return Err(format!("Unsupported file type in '{relative_path}'"));
    }
    if before.len() > max_file_bytes {
        return Err(format!("'{relative_path}' is too large for remote import"));
    }
    let projected_total = *total_bytes + before.len();
    if projected_total > max_total_bytes {
        return Err("Remote import is too large".to_owned());
    }
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err(format!(
            "File changed during upload staging: '{relative_path}'"
        ));
    }
    let mut buffer = Vec::with_capacity(opened.len() as usize);
    file.read_to_end(&mut buffer)
        .map_err(|error| error.to_string())?;
    if buffer.len() as u64 != opened.len() {
        return Err(format!(
            "File changed during upload staging: '{relative_path}'"
        ));
    }
    *total_bytes += opened.len();
    Ok(crate::fs_download_commands::encode_base64(&buffer))
}

/// `normalizeRelativeUploadPath`
/// (`filesystem-runtime-upload-staging.ts:212-214`).
fn normalize_relative_upload_path(path: &str) -> String {
    let collapsed: String = path.replace('\\', "/");
    collapsed.trim_start_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture_path(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mcode-tauri-fs-commands-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_repo_root(settings_path: &Path, id: &str, root: &Path) {
        std::fs::write(
            settings_path,
            serde_json::to_vec(&json!({
                "repos": [{ "id": id, "path": root.to_string_lossy() }]
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn filesystem_errors_keep_the_crate_stable_code_and_camel_case_shape() {
        let error = filesystem_error(FsError::PathDenied, FS_READ_DIR_OPERATION);

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "fs_path_denied",
                "context": { "operation": "fs_read_dir", "requestId": null, "intent": null },
                "message": "Access denied: path resolves outside allowed directories",
                "userMessage": "MCode could not list this folder. Try again."
            })
        );
    }

    #[test]
    fn every_fs_error_maps_onto_its_stable_code() {
        let cases = [
            (FsError::NotFound, "fs_not_found"),
            (FsError::AlreadyExists, "fs_already_exists"),
            (FsError::PermissionDenied, "fs_permission_denied"),
            (FsError::DestinationRace, "fs_destination_race"),
            (FsError::Cancelled, "fs_cancelled"),
            (FsError::IsDirectory, "fs_is_directory"),
        ];
        for (error, code) in cases {
            assert_eq!(filesystem_error(error, FS_STAT_OPERATION).code, code);
        }
    }

    #[test]
    fn request_paths_must_be_absolute_local_paths() {
        assert_eq!(
            local_path("relative/file.ts".to_owned(), None, FS_STAT_OPERATION)
                .unwrap_err()
                .code,
            "fs_invalid_argument"
        );
        assert_eq!(
            local_path(String::new(), None, FS_STAT_OPERATION)
                .unwrap_err()
                .code,
            "fs_invalid_argument"
        );
        assert_eq!(
            local_path(
                "D:/repo/file.ts".to_owned(),
                Some("ssh-1".to_owned()),
                FS_STAT_OPERATION
            )
            .unwrap_err()
            .code,
            "fs_remote_unsupported"
        );
        assert_eq!(
            local_path("D:/repo/file.ts".to_owned(), None, FS_STAT_OPERATION).unwrap(),
            PathBuf::from("D:/repo/file.ts")
        );
    }

    #[test]
    fn refresh_registers_settings_roots_and_revokes_removed_ones() {
        let fixture = fixture_path("refresh");
        let first_root = fixture.join("first");
        let replacement_root = fixture.join("replacement-with-a-different-length");
        std::fs::create_dir_all(&first_root).unwrap();
        std::fs::create_dir_all(&replacement_root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "first", &first_root);
        let (state, _receiver) =
            NativeFilesystemState::new(SettingsService::new(settings_path.clone()));

        assert!(state.probe_path_exists(&first_root).unwrap());

        write_repo_root(&settings_path, "replacement", &replacement_root);
        assert!(state.probe_path_exists(&replacement_root).unwrap());
        assert_eq!(
            state.probe_path_exists(&first_root).unwrap_err().code,
            "fs_path_denied"
        );

        drop(state);
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn settings_roots_that_fail_to_resolve_are_retried_on_the_next_request() {
        let fixture = fixture_path("retry");
        let delayed_root = fixture.join("delayed");
        std::fs::create_dir_all(&fixture).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "delayed", &delayed_root);
        let (state, _receiver) =
            NativeFilesystemState::new(SettingsService::new(settings_path.clone()));

        assert_eq!(
            state.probe_path_exists(&delayed_root).unwrap_err().code,
            "fs_path_denied"
        );

        std::fs::create_dir(&delayed_root).unwrap();
        assert!(state.probe_path_exists(&delayed_root).unwrap());

        drop(state);
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn watcher_subscriptions_are_released_with_their_window() {
        let fixture = fixture_path("watch");
        let root = fixture.join("root");
        std::fs::create_dir_all(&root).unwrap();
        let settings_path = fixture.join("mcode-data.json");
        write_repo_root(&settings_path, "scope", &root);
        let (state, _receiver) = NativeFilesystemState::new(SettingsService::new(settings_path));

        let window_id = window_scope_id_for_label("main");
        state.watch_root(window_id, &root).unwrap();
        assert!(
            state
                .0
                .window_subscriptions
                .lock()
                .unwrap()
                .get(&window_id)
                .is_some_and(|owned| owned.contains(&normalize_drive_case(&root)))
        );

        state.release_window(window_id);
        assert!(
            state
                .0
                .window_subscriptions
                .lock()
                .unwrap()
                .get(&window_id)
                .is_none_or(|owned| owned.is_empty())
        );

        drop(state);
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn different_window_labels_never_share_a_cancellation_scope() {
        assert_ne!(
            window_scope_id_for_label("main"),
            window_scope_id_for_label("secondary")
        );
    }

    #[test]
    fn stat_result_serializes_the_renderer_wire_shape() {
        let stat = ops::StatResult {
            size: 42,
            is_directory: false,
            mtime: 1_725_000_000_123,
        };
        assert_eq!(
            serde_json::to_value(stat).unwrap(),
            serde_json::json!({ "size": 42, "isDirectory": false, "mtime": 1_725_000_000_123i64 })
        );
    }

    #[test]
    fn read_file_result_serializes_the_renderer_wire_shape() {
        let read = ops::ReadFileResult {
            content: "hello".to_owned(),
            is_binary: false,
            mime_type: None,
        };
        assert_eq!(
            serde_json::to_value(read).unwrap(),
            json!({ "content": "hello", "isBinary": false })
        );
    }
}
