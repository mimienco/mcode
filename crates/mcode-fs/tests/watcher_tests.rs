//! Watcher semantics tests (spec §5): coalescing, batch cap, ignore rules,
//! drive normalization, unwatchable cache — all driven without sleeps by
//! exercising the pure pieces and small-timing windows.

use std::fs;
use std::path::Path;
use std::time::Duration;

use tempfile::TempDir;

use mcode_fs::watcher::{
    BatchWindow, WatchEvent, WatchEventKind, WatcherService, normalize_drive_case,
};

fn tiny_window() -> BatchWindow {
    BatchWindow {
        debounce: Duration::from_millis(30),
        max_wait: Duration::from_millis(120),
    }
}

// ---------- coalescing tests (pure classifier + flush behavior) ----------

// The coalescing rules live in `flush`, which is private; these tests drive
// the public surface: subscribe to a real tempdir, mutate, and read batches.
// The tiny window keeps tests deterministic (30 ms debounce / 120 ms max wait).

fn collect_batches(
    receiver: &crossbeam_channel::Receiver<mcode_fs::watcher::WatchBatch>,
    count: usize,
    timeout: Duration,
) -> Vec<mcode_fs::watcher::WatchBatch> {
    let mut batches = Vec::new();
    while batches.len() < count {
        match receiver.recv_timeout(timeout) {
            Ok(batch) => batches.push(batch),
            Err(_) => break,
        }
    }
    batches
}

#[test]
fn create_then_delete_in_one_window_nets_nothing() {
    let dir = TempDir::new().unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();

    let transient = dir.path().join("transient.txt");
    fs::write(&transient, "x").unwrap();
    fs::remove_file(&transient).unwrap();

    // Give the window a moment to close via max_wait; recv_timeout acts as
    // the wait, not a sleep-of-shame: we block exactly until a batch lands.
    let batches = collect_batches(&receiver, 1, Duration::from_millis(500));
    let events: Vec<&WatchEvent> = batches.iter().flat_map(|b| b.events.iter()).collect();
    let transient_events: Vec<_> = events
        .iter()
        .filter(|e| e.absolute_path.ends_with("transient.txt"))
        .collect();
    assert!(
        transient_events.is_empty(),
        "create→delete must net to nothing, got {transient_events:?}"
    );
}

#[test]
fn create_and_delete_of_separate_paths_both_survive() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("pre-existing.txt"), "old").unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();

    fs::write(dir.path().join("born.txt"), "new").unwrap();
    fs::remove_file(dir.path().join("pre-existing.txt")).unwrap();

    let batches = collect_batches(&receiver, 1, Duration::from_millis(500));
    let events: Vec<&WatchEvent> = batches.iter().flat_map(|b| b.events.iter()).collect();

    assert!(
        events
            .iter()
            .any(|e| e.absolute_path.ends_with("born.txt") && e.kind == WatchEventKind::Create),
        "create must survive: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e.absolute_path.ends_with("pre-existing.txt")
                && e.kind == WatchEventKind::Delete),
        "delete of a pre-window path must survive: {events:?}"
    );
}

#[test]
fn write_file_yields_update_event() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file.txt"), "v1").unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();

    fs::write(dir.path().join("file.txt"), "v2").unwrap();

    let batches = collect_batches(&receiver, 1, Duration::from_millis(500));
    let events: Vec<&WatchEvent> = batches.iter().flat_map(|b| b.events.iter()).collect();
    assert!(
        events
            .iter()
            .any(|e| e.absolute_path.ends_with("file.txt") && e.kind == WatchEventKind::Update),
        "expected an update event, got {events:?}"
    );
}

#[test]
fn ignored_names_produce_no_events() {
    let dir = TempDir::new().unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();

    fs::create_dir_all(dir.path().join("node_modules")).unwrap();
    fs::write(dir.path().join("node_modules").join("pkg.js"), "x").unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".git").join("HEAD"), "ref").unwrap();

    let batches = collect_batches(&receiver, 1, Duration::from_millis(400));
    let events: Vec<&WatchEvent> = batches.iter().flat_map(|b| b.events.iter()).collect();
    assert!(
        !events
            .iter()
            .any(|e| e.absolute_path.contains("node_modules")),
        "node_modules must be ignored, got {events:?}"
    );
    assert!(
        !events.iter().any(|e| e.absolute_path.contains(".git")),
        ".git must be ignored, got {events:?}"
    );
}

#[test]
fn multiple_windows_share_one_root_watcher() {
    let dir = TempDir::new().unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();
    // Second window subscribing to the same root must not spawn a second
    // native watcher nor error.
    service.subscribe(dir.path(), 2).unwrap();

    fs::write(dir.path().join("shared.txt"), "x").unwrap();
    let batches = collect_batches(&receiver, 1, Duration::from_millis(500));
    assert_eq!(batches.len(), 1, "one watcher per root emits one batch");
}

#[test]
fn unsubscribe_stops_events_for_last_owner() {
    let dir = TempDir::new().unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();
    service.unsubscribe(dir.path(), 1);

    // Give the teardown a beat, then mutate; no batch should arrive.
    fs::write(dir.path().join("after-unsub.txt"), "x").unwrap();
    let result = receiver.recv_timeout(Duration::from_millis(300));
    assert!(
        result.is_err(),
        "no events after unsubscribe, got {result:?}"
    );
}

#[test]
fn missing_root_is_unwatchable_and_cached() {
    let service = WatcherService::with_window(tiny_window()).0;
    let ghost = std::env::temp_dir().join("mcode-fs-definitely-missing-root-42");
    let error = service.subscribe(&ghost, 1).unwrap_err();
    assert_eq!(error.code(), "fs_not_found");
    assert!(service.is_unwatchable(&ghost), "failed root must be cached");
}

#[test]
fn batch_shape_serializes_camel_case() {
    let batch = mcode_fs::watcher::WatchBatch {
        worktree_path: "C:/repo".into(),
        events: vec![WatchEvent {
            kind: WatchEventKind::Create,
            absolute_path: "C:/repo/f.txt".into(),
            old_absolute_path: Some("C:/repo/old.txt".into()),
            is_directory: None,
        }],
    };
    let json = serde_json::to_value(&batch).unwrap();
    assert_eq!(json["worktreePath"], serde_json::json!("C:/repo"));
    assert_eq!(
        json["events"][0]["absolutePath"],
        serde_json::json!("C:/repo/f.txt")
    );
    assert_eq!(
        json["events"][0]["oldAbsolutePath"],
        serde_json::json!("C:/repo/old.txt")
    );
    assert_eq!(json["events"][0]["kind"], serde_json::json!("create"));
    // is_directory is undefined for deletes so the renderer infers from its
    // dir cache — omitted when None (spec §4/§5).
    assert!(json["events"][0].get("isDirectory").is_none());
}

#[test]
fn overflow_event_shape() {
    let event = WatchEvent {
        kind: WatchEventKind::Overflow,
        absolute_path: "C:/repo".into(),
        old_absolute_path: None,
        is_directory: Some(true),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["kind"], serde_json::json!("overflow"));
}

#[test]
fn drive_letter_normalization_uppercases_windows_drive() {
    let normalized = normalize_drive_case(Path::new("c:/repo/sub"));
    let text = normalized.to_string_lossy();
    assert!(text.starts_with('C'), "drive must be uppercase: {text}");
    assert!(
        text.contains("repo/sub"),
        "rest of path keeps casing: {text}"
    );
}

#[test]
fn event_absolute_paths_use_normalized_drive() {
    let dir = TempDir::new().unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();

    fs::write(dir.path().join("norm.txt"), "x").unwrap();
    let batches = collect_batches(&receiver, 1, Duration::from_millis(500));
    assert!(!batches.is_empty());
    for event in &batches[0].events {
        let text = &event.absolute_path;
        // Windows: the drive prefix of every event must be uppercase.
        let bytes = text.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' {
            assert!(
                bytes[0].is_ascii_uppercase(),
                "event drive must be normalized: {text}"
            );
        }
    }
}

#[test]
fn batch_cap_constant_matches_spec() {
    assert_eq!(mcode_fs::watcher::BATCH_CAP, 5000);
    assert_eq!(mcode_fs::watcher::DEBOUNCE, Duration::from_millis(150));
    assert_eq!(mcode_fs::watcher::MAX_WAIT, Duration::from_millis(500));
    assert_eq!(mcode_fs::watcher::MAX_WATCHERS, 256);
    assert_eq!(mcode_fs::watcher::UNWATCHABLE_CACHE_CAPACITY, 256);
    assert_eq!(
        mcode_fs::watcher::IGNORED_NAMES,
        [
            ".git",
            "node_modules",
            "dist",
            "build",
            ".next",
            ".cache",
            "target",
            ".venv",
            "__pycache__",
        ]
    );
}

#[test]
fn worktree_path_carries_root_in_batch() {
    let dir = TempDir::new().unwrap();
    let (service, receiver) = WatcherService::with_window(tiny_window());
    service.subscribe(dir.path(), 1).unwrap();

    fs::write(dir.path().join("rootcheck.txt"), "x").unwrap();
    let batches = collect_batches(&receiver, 1, Duration::from_millis(500));
    assert!(!batches.is_empty());
    let root_text = batches[0].worktree_path.clone();
    let expected = normalize_drive_case(dir.path())
        .to_string_lossy()
        .into_owned();
    assert_eq!(root_text, expected);
}
