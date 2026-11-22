//! Watcher service per spec §5 must-carry semantics.
//!
//! Transport-agnostic: events are delivered on a [`crossbeam_channel::Receiver`];
//! no tauri dependency lives in this crate. Semantics carried over:
//! trailing-edge debounce with a max-wait flush, per-path coalescing
//! (delete→create both; create→delete nets nothing unless the path predated
//! the window), batch cap with an `overflow` sentinel, ignore rules, Windows
//! drive-letter normalization, bounded watcher count with retry, an
//! unwatchable-root LRU, and per-`(root, window)` subscription ownership.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use notify::Watcher;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "watcher_lifecycle_tests.rs"]
mod lifecycle_tests;

/// Default trailing-edge debounce (spec §5, `filesystem-watch-batch-window.ts`).
pub const DEBOUNCE: Duration = Duration::from_millis(150);
/// Max-wait flush so a continuous event stream cannot starve the batch (spec §5).
pub const MAX_WAIT: Duration = Duration::from_millis(500);
/// Batch cap; above this one conservative `overflow` event is emitted (spec §5).
pub const BATCH_CAP: usize = 5000;
/// Bounded watcher count; subscribe retries when capacity frees (spec §5).
pub const MAX_WATCHERS: usize = 256;
/// Unwatchable-root LRU capacity (spec §5, `filesystem-watcher.ts:47-63`).
pub const UNWATCHABLE_CACHE_CAPACITY: usize = 256;

/// Names ignored by the watcher (spec §5 ignore rules).
pub const IGNORED_NAMES: [&str; 9] = [
    ".git",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".cache",
    "target",
    ".venv",
    "__pycache__",
];

/// One filesystem change (spec §4 `fs:changed` event payload item).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    pub kind: WatchEventKind,
    pub absolute_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_absolute_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_directory: Option<bool>,
}

/// Event kinds the renderer understands (spec §4 `fs:changed`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEventKind {
    Create,
    Update,
    Delete,
    Overflow,
}

/// A coalesced batch for one worktree root.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchBatch {
    pub worktree_path: String,
    pub events: Vec<WatchEvent>,
}

/// Tunable batch window so tests can shrink debounce without sleeping.
#[derive(Clone, Copy, Debug)]
pub struct BatchWindow {
    pub debounce: Duration,
    pub max_wait: Duration,
}

impl Default for BatchWindow {
    fn default() -> Self {
        Self {
            debounce: DEBOUNCE,
            max_wait: MAX_WAIT,
        }
    }
}

#[derive(Debug)]
struct PendingBatch {
    #[allow(dead_code)]
    started: Instant,
    last_event: Instant,
    events: VecDeque<(PathBuf, WatchEvent)>,
    pre_window: HashSet<PathBuf>,
    overflowed: bool,
}

impl PendingBatch {
    fn new(now: Instant) -> Self {
        Self {
            started: now,
            last_event: now,
            events: VecDeque::new(),
            pre_window: HashSet::new(),
            overflowed: false,
        }
    }
}

/// Normalizes a Windows drive-letter root/event path to an uppercase drive
/// (spec §5, `filesystem-watcher.ts:186-207`).
pub fn normalize_drive_case(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let mut owned = text.to_uppercase();
        // Only fold the drive letter; the rest keeps its own casing.
        owned.replace_range(2.., &text[2..]);
        // `to_uppercase` uppercased the whole string; rebuild properly.
        let mut rebuilt = String::with_capacity(text.len());
        rebuilt.push(bytes[0].to_ascii_uppercase() as char);
        rebuilt.push(':');
        rebuilt.push_str(&text[2..]);
        return PathBuf::from(rebuilt);
    }
    PathBuf::from(text.into_owned())
}

fn is_ignored(name: &str) -> bool {
    IGNORED_NAMES.contains(&name)
}

/// True when any component of `path` matches an ignore rule (spec §5:
/// nested-dir ignore, component-aware).
fn path_has_ignored_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        std::path::Component::Normal(name) => is_ignored(&name.to_string_lossy()),
        _ => false,
    })
}

/// Maps a raw notify event onto the wire event kinds, preserving renames as
/// delete(old)+create(new) pairs since the wire contract has no rename kind
/// beyond `oldAbsolutePath`.
fn classify(event: &notify::Event) -> Vec<(PathBuf, WatchEvent)> {
    use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode};
    let mut out = Vec::new();
    let is_directory_flag: Option<bool> = None;
    for path in &event.paths {
        let normalized = normalize_drive_case(path);
        if path_has_ignored_component(&normalized) {
            continue;
        }
        {
            let kind = &event.kind;
            let wire = match kind {
                notify::EventKind::Create(
                    CreateKind::Folder | CreateKind::File | CreateKind::Any | CreateKind::Other,
                ) => Some(WatchEvent {
                    kind: WatchEventKind::Create,
                    absolute_path: normalized.to_string_lossy().into_owned(),
                    old_absolute_path: None,
                    is_directory: is_directory_flag,
                }),
                notify::EventKind::Modify(
                    ModifyKind::Data(DataChange::Any | DataChange::Other | DataChange::Size)
                    | ModifyKind::Metadata(_)
                    | ModifyKind::Any
                    | ModifyKind::Other,
                ) => Some(WatchEvent {
                    kind: WatchEventKind::Update,
                    absolute_path: normalized.to_string_lossy().into_owned(),
                    old_absolute_path: None,
                    is_directory: is_directory_flag,
                }),
                notify::EventKind::Modify(ModifyKind::Name(RenameMode::From)) => Some(WatchEvent {
                    kind: WatchEventKind::Delete,
                    absolute_path: normalized.to_string_lossy().into_owned(),
                    old_absolute_path: None,
                    is_directory: is_directory_flag,
                }),
                notify::EventKind::Modify(ModifyKind::Name(RenameMode::To)) => Some(WatchEvent {
                    kind: WatchEventKind::Create,
                    absolute_path: normalized.to_string_lossy().into_owned(),
                    old_absolute_path: None,
                    is_directory: is_directory_flag,
                }),
                notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                    // Rename is expressed as delete(old)+create(new).
                    let old = event
                        .paths
                        .first()
                        .cloned()
                        .map(|p| normalize_drive_case(&p));
                    let new = event
                        .paths
                        .get(1)
                        .cloned()
                        .map(|p| normalize_drive_case(&p));
                    if let (Some(old_path), Some(new_path)) = (old, new) {
                        out.push((
                            old_path.clone(),
                            WatchEvent {
                                kind: WatchEventKind::Delete,
                                absolute_path: old_path.to_string_lossy().into_owned(),
                                old_absolute_path: None,
                                is_directory: is_directory_flag,
                            },
                        ));
                        out.push((
                            new_path.clone(),
                            WatchEvent {
                                kind: WatchEventKind::Create,
                                absolute_path: new_path.to_string_lossy().into_owned(),
                                old_absolute_path: Some(old_path.to_string_lossy().into_owned()),
                                is_directory: is_directory_flag,
                            },
                        ));
                    }
                    None
                }
                notify::EventKind::Remove(
                    RemoveKind::File | RemoveKind::Folder | RemoveKind::Any | RemoveKind::Other,
                ) => Some(WatchEvent {
                    kind: WatchEventKind::Delete,
                    absolute_path: normalized.to_string_lossy().into_owned(),
                    old_absolute_path: None,
                    is_directory: is_directory_flag,
                }),
                _ => None,
            };
            if let Some(wire) = wire {
                out.push((normalized.clone(), wire));
            }
        }
    }
    out
}

/// Per-(root, window) subscription ownership (spec §5).
#[derive(Debug)]
pub struct WatcherService {
    emitter: Sender<WatchBatch>,
    window: BatchWindow,
    active_watchers: Arc<AtomicUsize>,
    pending: Arc<Mutex<HashMap<String, PendingBatch>>>,
    unwatchable: Arc<Mutex<std::collections::VecDeque<String>>>,
    subscriptions: Mutex<HashMap<String, NativeSubscription>>,
}

#[derive(Debug)]
struct NativeSubscription {
    owners: HashSet<u64>,
    shutdown: Sender<()>,
    thread: thread::JoinHandle<()>,
}

impl NativeSubscription {
    fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.thread.join();
    }
}

impl Drop for WatcherService {
    fn drop(&mut self) {
        let subscriptions = self.subscriptions.get_mut().expect("subscriptions lock");
        for subscription in subscriptions.values() {
            let _ = subscription.shutdown.try_send(());
        }
        for (_, subscription) in subscriptions.drain() {
            let _ = subscription.thread.join();
        }
    }
}

impl WatcherService {
    /// Creates the service; batches flow on the returned [`Receiver`].
    pub fn new() -> (Self, Receiver<WatchBatch>) {
        let (emitter, receiver) = crossbeam_channel::unbounded();
        let service = Self {
            emitter,
            window: BatchWindow::default(),
            active_watchers: Arc::new(AtomicUsize::new(0)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            unwatchable: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            subscriptions: Mutex::new(HashMap::new()),
        };
        (service, receiver)
    }

    /// Installs the service with custom timing for deterministic tests.
    pub fn with_window(window: BatchWindow) -> (Self, Receiver<WatchBatch>) {
        let (emitter, receiver) = crossbeam_channel::unbounded();
        let service = Self {
            emitter,
            window,
            active_watchers: Arc::new(AtomicUsize::new(0)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            unwatchable: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            subscriptions: Mutex::new(HashMap::new()),
        };
        (service, receiver)
    }

    /// Whether `root` previously failed to install and is still cached.
    pub fn is_unwatchable(&self, root: &Path) -> bool {
        let key = normalize_drive_case(root).to_string_lossy().into_owned();
        let cache = self.unwatchable.lock().expect("unwatchable lock");
        cache.iter().any(|entry| entry == &key)
    }

    fn mark_unwatchable(&self, root: &Path) {
        let key = normalize_drive_case(root).to_string_lossy().into_owned();
        let mut cache = self.unwatchable.lock().expect("unwatchable lock");
        if cache.iter().any(|entry| entry == &key) {
            return;
        }
        if cache.len() >= UNWATCHABLE_CACHE_CAPACITY {
            cache.pop_front();
        }
        cache.push_back(key);
    }

    /// Subscribes `window_id` to `root`, spawning the notify watcher and the
    /// debounce/coalesce pump.
    ///
    /// Reaching [`MAX_WATCHERS`] returns an error, and the caller is expected
    /// to retry when capacity frees (spec §5 bounded capacity semantics). An
    /// unreadable root marks the LRU cache and errors.
    pub fn subscribe(&self, root: &Path, window_id: u64) -> Result<(), crate::scope::FsError> {
        let normalized_root = normalize_drive_case(root);
        let key = normalized_root.to_string_lossy().into_owned();
        let mut subscriptions = self.subscriptions.lock().expect("subscriptions lock");
        if let Some(subscription) = subscriptions.get_mut(&key)
            && !subscription.thread.is_finished()
        {
            subscription.owners.insert(window_id);
            return Ok(());
        }
        // Preserve the other owners when retrying a failed native watcher.
        let mut owners = match subscriptions.remove(&key) {
            Some(subscription) => {
                let owners = subscription.owners.clone();
                subscription.stop();
                owners
            }
            None => HashSet::new(),
        };
        if fs::metadata(&normalized_root).is_err() {
            self.mark_unwatchable(&normalized_root);
            return Err(crate::scope::FsError::NotFound);
        }
        if self.active_watchers.load(Ordering::SeqCst) >= MAX_WATCHERS {
            return Err(crate::scope::FsError::Io("watcher capacity reached".into()));
        }
        self.active_watchers.fetch_add(1, Ordering::SeqCst);
        match self.spawn_native(&normalized_root, key.clone()) {
            Ok(mut subscription) => {
                owners.insert(window_id);
                subscription.owners = owners;
                subscriptions.insert(key, subscription);
                Ok(())
            }
            Err(error) => {
                self.active_watchers.fetch_sub(1, Ordering::SeqCst);
                self.mark_unwatchable(&normalized_root);
                Err(error)
            }
        }
    }

    /// Drops only this window's ownership; the last owner stops the pump.
    pub fn unsubscribe(&self, root: &Path, window_id: u64) {
        let key = normalize_drive_case(root).to_string_lossy().into_owned();
        let mut subscriptions = self.subscriptions.lock().expect("subscriptions lock");
        let Some(subscription) = subscriptions.get_mut(&key) else {
            return;
        };
        if !subscription.owners.remove(&window_id) || !subscription.owners.is_empty() {
            return;
        }
        if let Some(subscription) = subscriptions.remove(&key) {
            // Serialize teardown with a new subscription to this same root.
            subscription.stop();
        }
    }

    fn spawn_native(
        &self,
        root: &Path,
        root_key: String,
    ) -> Result<NativeSubscription, crate::scope::FsError> {
        let (notify_tx, notify_rx) =
            crossbeam_channel::unbounded::<notify::Result<notify::Event>>();
        let mut watcher = notify::RecommendedWatcher::new(
            move |result| {
                let _ = notify_tx.send(result);
            },
            notify::Config::default(),
        )
        .map_err(|error| crate::scope::FsError::Io(error.to_string()))?;
        watcher
            .watch(root, notify::RecursiveMode::Recursive)
            .map_err(|error| crate::scope::FsError::Io(error.to_string()))?;

        // The watcher handle lives in the pump thread; dropping it (or being
        // dropped at thread exit) stops the event stream, which also serves
        // as the unsubscribe teardown path.
        let pending = Arc::clone(&self.pending);
        let emitter = self.emitter.clone();
        let window = self.window;
        let root_for_thread = root.to_path_buf();
        let active = Arc::clone(&self.active_watchers);
        let (pump_tx, pump_rx) = crossbeam_channel::bounded::<()>(1);
        let thread = thread::Builder::new()
            .name(format!("mcode-fs-watcher-{root_key}"))
            .spawn(move || {
                let _keep_watcher = watcher;
                let mut shutdown = Some(pump_rx);
                pump(
                    &root_for_thread,
                    root_key.clone(),
                    &notify_rx,
                    &pending,
                    &emitter,
                    window,
                    &mut shutdown,
                );
                pending.lock().expect("pending lock").remove(&root_key);
                drop(_keep_watcher);
                active.fetch_sub(1, Ordering::SeqCst);
            })
            .map_err(|error| crate::scope::FsError::Io(error.to_string()))?;
        Ok(NativeSubscription {
            owners: HashSet::new(),
            shutdown: pump_tx,
            thread,
        })
    }
}

/// Drives the debounce/coalesce loop for one root.
fn pump(
    root: &Path,
    root_key: String,
    notify_rx: &Receiver<notify::Result<notify::Event>>,
    pending: &Arc<Mutex<HashMap<String, PendingBatch>>>,
    emitter: &Sender<WatchBatch>,
    window: BatchWindow,
    shutdown: &mut Option<Receiver<()>>,
) {
    let mut batch_open: Option<Instant> = None;
    loop {
        let timeout = match batch_open {
            Some(started) => {
                let elapsed = started.elapsed();
                if elapsed >= window.max_wait {
                    flush(root, &root_key, pending, emitter, &mut batch_open);
                    continue;
                }
                Some(window.max_wait - elapsed)
            }
            None => Some(Duration::from_millis(200)),
        };
        if let Some(receiver) = shutdown
            && !matches!(
                receiver.try_recv(),
                Err(crossbeam_channel::TryRecvError::Empty)
            )
        {
            flush(root, &root_key, pending, emitter, &mut batch_open);
            return;
        }
        match notify_rx.recv_timeout(timeout.unwrap_or(Duration::from_millis(200))) {
            Ok(Ok(event)) => {
                let now = Instant::now();
                let mut guard = pending.lock().expect("pending lock");
                let entry = guard.entry(root_key.clone()).or_insert_with(|| {
                    batch_open = Some(now);
                    PendingBatch::new(now)
                });
                for (path, wire) in classify(&event) {
                    if entry.events.len() >= BATCH_CAP {
                        entry.overflowed = true;
                        break;
                    }
                    entry.last_event = now;
                    entry.events.push_back((path, wire));
                }
            }
            Ok(Err(_)) => {
                // Watcher error becomes a conservative overflow (spec §5).
                emit_overflow(root, &root_key, emitter);
                return;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if batch_open.is_some() {
                    flush(root, &root_key, pending, emitter, &mut batch_open);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Coalesces and emits the pending batch for `root_key`.
fn flush(
    root: &Path,
    root_key: &str,
    pending: &Arc<Mutex<HashMap<String, PendingBatch>>>,
    emitter: &Sender<WatchBatch>,
    batch_open: &mut Option<Instant>,
) {
    let mut guard = pending.lock().expect("pending lock");
    let Some(entry) = guard.remove(root_key) else {
        *batch_open = None;
        return;
    };
    *batch_open = None;
    drop(guard);

    if entry.overflowed || entry.events.len() > BATCH_CAP {
        emit_overflow(root, root_key, emitter);
        return;
    }

    // Per-path coalescing: last event wins, delete→create keeps both,
    // create→delete nets to nothing unless the path predated the window
    // (spec §5, `filesystem-watcher.ts:232-292`).
    let mut by_path: HashMap<String, WatchEvent> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut deletions: HashSet<String> = HashSet::new();
    let mut creations: HashSet<String> = HashSet::new();
    for (_, event) in entry.events {
        let path_key = event.absolute_path.clone();
        match event.kind {
            WatchEventKind::Delete => {
                deletions.insert(path_key.clone());
                if creations.contains(&path_key) && !entry.pre_window.contains(Path::new(&path_key))
                {
                    // create→delete nets to nothing for a brand-new path.
                    by_path.remove(&path_key);
                    order.retain(|p| p != &path_key);
                    creations.remove(&path_key);
                    deletions.remove(&path_key);
                    continue;
                }
                by_path.insert(path_key.clone(), event);
                if !order.contains(&path_key) {
                    order.push(path_key);
                }
            }
            WatchEventKind::Create => {
                creations.insert(path_key.clone());
                by_path.insert(path_key.clone(), event);
                if !order.contains(&path_key) {
                    order.push(path_key);
                }
            }
            WatchEventKind::Update => {
                // A Create is never demoted to an Update within one window:
                // on Windows ReadDirectoryChangesW emits Modify(Any) right
                // after Create for the same new file, and the renderer must
                // still see the create.
                if by_path
                    .get(&path_key)
                    .map(|existing| existing.kind == WatchEventKind::Create)
                    .unwrap_or(false)
                {
                    continue;
                }
                by_path.insert(path_key.clone(), event);
                if !order.contains(&path_key) {
                    order.push(path_key);
                }
            }
            WatchEventKind::Overflow => unreachable!("overflow is synthesized, not classified"),
        }
    }
    let events = order
        .into_iter()
        .filter_map(|path| by_path.remove(&path))
        .collect();
    let _ = deletions;
    let _ = emitter.send(WatchBatch {
        worktree_path: root.to_string_lossy().into_owned(),
        events,
    });
}

/// Emits a single conservative overflow event for the root (spec §5).
fn emit_overflow(root: &Path, root_key: &str, emitter: &Sender<WatchBatch>) {
    let _ = root_key;
    let _ = emitter.send(WatchBatch {
        worktree_path: root.to_string_lossy().into_owned(),
        events: vec![WatchEvent {
            kind: WatchEventKind::Overflow,
            absolute_path: root.to_string_lossy().into_owned(),
            old_absolute_path: None,
            is_directory: Some(true),
        }],
    });
}
