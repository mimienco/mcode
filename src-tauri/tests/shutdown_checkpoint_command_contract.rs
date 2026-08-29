use mcode_desktop::app_stage_shutdown_checkpoint_for;
use mcode_settings::{SettingsService, ShutdownCheckpoint};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mcode-tauri-checkpoint-command-{}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let expected = format!("mcode-tauri-checkpoint-command-{}", std::process::id());
        let is_owned_test_directory = self.0.file_name().and_then(|name| name.to_str())
            == Some(expected.as_str())
            && self.0.parent() == Some(std::env::temp_dir().as_path());
        if is_owned_test_directory {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn renderer_json_contract_reaches_the_atomic_settings_boundary() {
    let directory = TestDirectory::new();
    let settings_path = directory.0.join("mcode-data.json");
    fs::write(
        &settings_path,
        br#"{"settings":{"theme":"dark"},"ui":{"sidebarWidth":320}}"#,
    )
    .unwrap();
    let service = SettingsService::new(settings_path.clone());
    let checkpoint: ShutdownCheckpoint = serde_json::from_value(json!({
        "version": 1,
        "sessions": [{
            "state": { "activeWorktreeId": "worktree-1" },
            "hostId": "runtime:host-1"
        }],
        "ui": { "activeView": "terminal" }
    }))
    .unwrap();

    app_stage_shutdown_checkpoint_for(&service, checkpoint).unwrap();

    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(settings_path).unwrap()).unwrap();
    assert_eq!(persisted["settings"], json!({ "theme": "dark" }));
    assert_eq!(
        persisted["ui"],
        json!({ "activeView": "terminal", "sidebarWidth": 320 })
    );
    assert_eq!(
        persisted["workspaceSessionsByHostId"]["runtime:host-1"],
        json!({ "activeWorktreeId": "worktree-1" })
    );
}
