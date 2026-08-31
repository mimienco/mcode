//! Tauri commands over the `mcode-fs` scoped filesystem crate (spec
//! `docs/migration/FS_CONTRACT_SPEC.md`).
//!
//! Every command authorizes the requested path through
//! [`FsScopeRegistry::authorize`] before touching the disk, seeds the
//! allow-list from `SettingsService::local_filesystem_roots()`, and maps
//! [`FsError`] onto the shared [`DesktopCommandError`] shape with the crate's
//! stable snake_case codes (`fs_path_denied`, ...).

use crossbeam_channel::Receiver;
use mcode_fs::ops;
use mcode_fs::scope::{FsError, FsScopeRegistry, LeafPolicy};
use mcode_fs::search::{self, CancellationRegistry};
use mcode_fs::watcher::{WatchBatch, WatcherService, normalize_drive_case};
use mcode_settings::{SettingsService, SettingsServiceError};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, WebviewWindow};

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

/// Renderer-facing watcher event name (spec §4 `fs:changed`).
pub(crate) const FS_CHANGED_EVENT: &str = "fs://changed";

struct NativeFilesystemStateInner {
    registry: FsScopeRegistry,
    settings: SettingsService,
    watchers: WatcherService,
    cancellations: CancellationRegistry,
    cached_roots: Mutex<Option<(Vec<PathBuf>, Vec<PathBuf>)>>,
    window_subscriptions: Mutex<HashMap<u64, HashSet<PathBuf>>>,
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
