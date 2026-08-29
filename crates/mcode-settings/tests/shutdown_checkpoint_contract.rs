use mcode_settings::{
    CheckpointError, SettingsService, ShutdownCheckpoint, ShutdownSession,
    apply_shutdown_checkpoint,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mcode-settings-contract-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let expected_prefix = format!("mcode-settings-contract-{}-", std::process::id());
        let is_owned_test_directory = self
            .0
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&expected_prefix))
            && self.0.parent() == Some(std::env::temp_dir().as_path());
        if is_owned_test_directory {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn local_session_and_ui_patch_preserve_unrelated_persisted_state() {
    let existing = json!({
        "settings": { "theme": "dark" },
        "ui": { "activeView": "terminal", "sidebarWidth": 320 },
        "workspaceSession": { "activeWorktreeId": "old" }
    });
    let checkpoint = ShutdownCheckpoint {
        version: 1,
        sessions: vec![ShutdownSession {
            host_id: None,
            state: json!({ "activeWorktreeId": "new" }),
        }],
        ui: serde_json::from_value(json!({ "activeView": "settings" })).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();

    assert_eq!(merged["settings"], json!({ "theme": "dark" }));
    assert_eq!(
        merged["ui"],
        json!({ "activeView": "settings", "sidebarWidth": 320 })
    );
    assert_eq!(
        merged["workspaceSession"],
        json!({ "activeWorktreeId": "new" })
    );
}

#[test]
fn host_session_updates_its_partition_without_replacing_the_local_session() {
    let existing = json!({
        "ui": {},
        "workspaceSession": { "activeWorktreeId": "local" },
        "workspaceSessionsByHostId": {
            "runtime:other": { "activeWorktreeId": "other" }
        }
    });
    let checkpoint = ShutdownCheckpoint {
        version: 1,
        sessions: vec![ShutdownSession {
            host_id: Some("runtime:host-1".into()),
            state: json!({ "activeWorktreeId": "remote" }),
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();

    assert_eq!(
        merged["workspaceSession"],
        json!({ "activeWorktreeId": "local" })
    );
    assert_eq!(
        merged["workspaceSessionsByHostId"]["runtime:host-1"],
        json!({ "activeWorktreeId": "remote" })
    );
    assert_eq!(
        merged["workspaceSessionsByHostId"]["runtime:other"],
        json!({ "activeWorktreeId": "other" })
    );
}

#[test]
fn rejects_an_unknown_checkpoint_contract_version() {
    let checkpoint = ShutdownCheckpoint {
        version: 2,
        sessions: vec![],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let result = apply_shutdown_checkpoint(json!({}), checkpoint);

    assert_eq!(result, Err(CheckpointError::UnsupportedVersion(2)));
}

#[test]
fn settings_service_atomically_merges_a_shutdown_checkpoint() {
    let directory = TestDirectory::new();
    let settings_path = directory.join("nested/mcode-data.json");
    fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    fs::write(
        &settings_path,
        br#"{"settings":{"theme":"dark"},"ui":{"sidebarWidth":320}}"#,
    )
    .unwrap();
    let service = SettingsService::new(settings_path.clone());

    service
        .stage_shutdown_checkpoint(ShutdownCheckpoint {
            version: 1,
            sessions: vec![ShutdownSession {
                host_id: None,
                state: json!({ "activeWorktreeId": "worktree-1" }),
            }],
            ui: serde_json::from_value(json!({ "activeView": "terminal" })).unwrap(),
        })
        .unwrap();

    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(settings_path).unwrap()).unwrap();
    assert_eq!(persisted["settings"], json!({ "theme": "dark" }));
    assert_eq!(
        persisted["ui"],
        json!({ "activeView": "terminal", "sidebarWidth": 320 })
    );
    assert_eq!(
        persisted["workspaceSession"],
        json!({ "activeWorktreeId": "worktree-1" })
    );
}

#[test]
fn settings_service_keeps_existing_bytes_when_checkpoint_validation_fails() {
    let directory = TestDirectory::new();
    let settings_path = directory.join("mcode-data.json");
    let original = br#"{"settings":{"theme":"dark"}}"#;
    fs::write(&settings_path, original).unwrap();
    let service = SettingsService::new(settings_path.clone());

    let result = service.stage_shutdown_checkpoint(ShutdownCheckpoint {
        version: 2,
        sessions: vec![],
        ui: serde_json::from_value(json!({})).unwrap(),
    });

    assert!(result.is_err());
    assert_eq!(fs::read(settings_path).unwrap(), original);
}

#[test]
fn settings_service_keeps_malformed_existing_settings_untouched() {
    let directory = TestDirectory::new();
    let settings_path = directory.join("mcode-data.json");
    let original = br#"{"settings": "#;
    fs::write(&settings_path, original).unwrap();
    let service = SettingsService::new(settings_path.clone());

    let result = service.stage_shutdown_checkpoint(ShutdownCheckpoint {
        version: 1,
        sessions: vec![],
        ui: serde_json::from_value(json!({ "activeView": "terminal" })).unwrap(),
    });

    assert!(result.is_err());
    assert_eq!(fs::read(settings_path).unwrap(), original);
}
