use mcode_settings::{
    CheckpointError, ExecutionHostId, SettingsService, ShutdownCheckpoint, ShutdownSession,
    WorkspaceSessionState, apply_shutdown_checkpoint,
};
use serde_json::json;
use std::fs;
use std::fs::File;
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

fn workspace_session(active_worktree_id: Option<&str>) -> WorkspaceSessionState {
    serde_json::from_value(json!({
        "activeRepoId": null,
        "activeWorktreeId": active_worktree_id,
        "activeTabId": null,
        "tabsByWorktree": {},
        "terminalLayoutsByTabId": {}
    }))
    .unwrap()
}

const RECONCILIATION_LEAF_ID: &str = "11111111-1111-4111-8111-111111111111";

fn terminal_binding_session(pty_id: Option<&str>) -> WorkspaceSessionState {
    let bindings = pty_id
        .map(|value| json!({ (RECONCILIATION_LEAF_ID): value }))
        .unwrap_or_else(|| json!({}));
    let buffers = pty_id
        .map(|_| json!({ (RECONCILIATION_LEAF_ID): "buffer" }))
        .unwrap_or_else(|| json!({}));
    let scrollback_refs = pty_id
        .map(|_| json!({ (RECONCILIATION_LEAF_ID): "scrollback-1" }))
        .unwrap_or_else(|| json!({}));
    let titles = pty_id
        .map(|_| json!({ (RECONCILIATION_LEAF_ID): "Pane title" }))
        .unwrap_or_else(|| json!({}));
    serde_json::from_value(json!({
        "activeRepoId": "repo-1",
        "activeWorktreeId": "repo-1::D:/workspace",
        "activeTabId": "tab-1",
        "tabsByWorktree": {
            "repo-1::D:/workspace": [{
                "id": "tab-1",
                "ptyId": pty_id,
                "worktreeId": "repo-1::D:/workspace",
                "title": "Terminal",
                "customTitle": null,
                "color": null,
                "sortOrder": 0,
                "createdAt": 1
            }]
        },
        "terminalLayoutsByTabId": {
            "tab-1": {
                "root": { "type": "leaf", "leafId": RECONCILIATION_LEAF_ID },
                "activeLeafId": RECONCILIATION_LEAF_ID,
                "expandedLeafId": null,
                "ptyIdsByLeafId": bindings,
                "buffersByLeafId": buffers,
                "scrollbackRefsByLeafId": scrollback_refs,
                "titlesByLeafId": titles
            }
        }
    }))
    .unwrap()
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
            state: workspace_session(Some("new")),
        }],
        ui: serde_json::from_value(json!({ "activeView": "settings" })).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();

    assert_eq!(merged["settings"], json!({ "theme": "dark" }));
    assert_eq!(
        merged["ui"],
        json!({ "activeView": "settings", "sidebarWidth": 320 })
    );
    assert_eq!(merged["workspaceSession"]["activeWorktreeId"], json!("new"));
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
            host_id: Some(ExecutionHostId::parse("runtime:host-1").unwrap()),
            state: workspace_session(Some("remote")),
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();

    assert_eq!(
        merged["workspaceSession"],
        json!({ "activeWorktreeId": "local" })
    );
    assert_eq!(
        merged["workspaceSessionsByHostId"]["runtime:host-1"]["activeWorktreeId"],
        json!("remote")
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
fn rejects_untyped_workspace_session_payloads_at_deserialization() {
    let checkpoint = serde_json::from_value::<ShutdownCheckpoint>(json!({
        "version": 1,
        "sessions": [{ "state": "not-a-session" }],
        "ui": {}
    }));

    assert!(checkpoint.is_err());
}

#[test]
fn rejects_structurally_invalid_workspace_sessions_at_deserialization() {
    let checkpoint = serde_json::from_value::<ShutdownCheckpoint>(json!({
        "version": 1,
        "sessions": [{
            "state": {
                "activeRepoId": null,
                "activeWorktreeId": null,
                "activeTabId": null,
                "tabsByWorktree": "not-a-record",
                "terminalLayoutsByTabId": {}
            }
        }],
        "ui": {}
    }));

    assert!(checkpoint.is_err());
}

#[test]
fn rejects_invalid_nested_workspace_session_records() {
    let checkpoint = serde_json::from_value::<ShutdownCheckpoint>(json!({
        "version": 1,
        "sessions": [{
            "state": {
                "activeRepoId": null,
                "activeWorktreeId": null,
                "activeTabId": null,
                "tabsByWorktree": { "worktree-1": [{}] },
                "terminalLayoutsByTabId": {}
            }
        }],
        "ui": {}
    }));

    assert!(checkpoint.is_err());
}

#[test]
fn shared_v1_fixture_covers_the_rust_checkpoint_contract() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/shutdown-checkpoint-v1.json")).unwrap();

    serde_json::from_value::<ShutdownCheckpoint>(fixture["checkpoint"].clone()).unwrap();

    for active_view in fixture["topLevelViews"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["ui"]["activeView"] = active_view.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for agent in fixture["tuiAgents"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["tabsByWorktree"]["worktree-1"][0]["launchAgent"] =
            agent.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for agent in fixture["resumableTuiAgents"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        let session = &mut checkpoint["sessions"][0]["state"]["sleepingAgentSessionsByPaneKey"]["tab-1:leaf-1"];
        session["agent"] = agent.clone();
        session["providerSession"]["key"] = if agent == "antigravity" {
            json!("conversation_id")
        } else {
            json!("session_id")
        };
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for direction in fixture["terminalPaneSplitDirections"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["terminalLayoutsByTabId"]["tab-1"]["root"] = json!({
            "type": "split",
            "direction": direction,
            "first": { "type": "leaf", "leafId": "leaf-1" },
            "second": { "type": "leaf", "leafId": "leaf-2" }
        });
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for direction in fixture["tabGroupSplitDirections"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["tabGroupLayouts"]["worktree-1"] = json!({
            "type": "split",
            "direction": direction,
            "first": { "type": "leaf", "groupId": "group-1" },
            "second": { "type": "leaf", "groupId": "group-2" }
        });
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for content_type in fixture["tabContentTypes"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["unifiedTabs"]["worktree-1"][0]["contentType"] =
            content_type.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for visible_type in fixture["workspaceVisibleTabTypes"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["activeTabTypeByWorktree"]["worktree-1"] =
            visible_type.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for view_mode in fixture["tabViewModes"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["unifiedTabs"]["worktree-1"][0]["viewMode"] =
            view_mode.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for agent in fixture["aiVaultTitleAgents"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["tabsByWorktree"]["worktree-1"][0]["aiVaultTitle"]["agent"] =
            agent.clone();
        checkpoint["sessions"][0]["state"]["unifiedTabs"]["worktree-1"][0]["aiVaultTitle"] = json!({
            "agent": agent,
            "sessionId": "session-1",
            "title": "Parity session"
        });
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for preset in fixture["browserViewportPresetIds"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["browserPagesByWorkspace"]["browser-workspace-1"][0]["viewportPresetId"] =
            preset.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for key in fixture["providerSessionKeys"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        let session = &mut checkpoint["sessions"][0]["state"]["sleepingAgentSessionsByPaneKey"]["tab-1:leaf-1"];
        session["providerSession"]["key"] = key.clone();
        session["agent"] = if key == "conversation_id" {
            json!("antigravity")
        } else {
            json!("codex")
        };
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for state in fixture["sleepingAgentStates"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["sleepingAgentSessionsByPaneKey"]["tab-1:leaf-1"]["state"] =
            state.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for origin in fixture["sleepingAgentOrigins"].as_array().unwrap() {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["sleepingAgentSessionsByPaneKey"]["tab-1:leaf-1"]["origin"] =
            origin.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for reason in fixture["automaticResumeBlockedByValues"]
        .as_array()
        .unwrap()
    {
        let mut checkpoint = fixture["checkpoint"].clone();
        checkpoint["sessions"][0]["state"]["sleepingAgentSessionsByPaneKey"]["tab-1:leaf-1"]["automaticResumeBlockedBy"] =
            reason.clone();
        serde_json::from_value::<ShutdownCheckpoint>(checkpoint).unwrap();
    }

    for host_id in fixture["executionHostIds"].as_array().unwrap() {
        ExecutionHostId::parse(host_id.as_str().unwrap()).unwrap();
    }
}

#[test]
fn execution_host_ids_share_the_renderer_normalization_boundary() {
    assert_eq!(
        ExecutionHostId::parse("  ssh:target-1  ").unwrap(),
        ExecutionHostId::parse("ssh:target-1").unwrap()
    );
    for invalid in ["", "all", "ssh:", "runtime:", "ssh:%", "runtime:%FF"] {
        assert!(
            ExecutionHostId::parse(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[test]
fn local_checkpoint_preserves_restorable_bindings_from_a_stale_renderer_snapshot() {
    let existing = json!({
        "ui": {},
        "workspaceSession": serde_json::to_value(terminal_binding_session(Some("pty-1"))).unwrap()
    });
    let checkpoint = ShutdownCheckpoint {
        version: 1,
        sessions: vec![ShutdownSession {
            host_id: None,
            state: terminal_binding_session(None),
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();
    let tab = &merged["workspaceSession"]["tabsByWorktree"]["repo-1::D:/workspace"][0];
    let layout = &merged["workspaceSession"]["terminalLayoutsByTabId"]["tab-1"];

    assert_eq!(tab["ptyId"], json!("pty-1"));
    assert_eq!(
        layout["ptyIdsByLeafId"][RECONCILIATION_LEAF_ID],
        json!("pty-1")
    );
    assert_eq!(
        layout["buffersByLeafId"][RECONCILIATION_LEAF_ID],
        json!("buffer")
    );
    assert_eq!(
        layout["scrollbackRefsByLeafId"][RECONCILIATION_LEAF_ID],
        json!("scrollback-1")
    );
    assert_eq!(
        layout["titlesByLeafId"][RECONCILIATION_LEAF_ID],
        json!("Pane title")
    );
}

#[test]
fn local_checkpoint_does_not_resurrect_a_terminated_ssh_binding() {
    let existing = json!({
        "ui": {},
        "repos": [{ "id": "repo-1", "connectionId": "ssh-target" }],
        "sshRemotePtyLeases": [{
            "targetId": "ssh-target",
            "ptyId": "remote-pty",
            "worktreeId": "repo-1::D:/workspace",
            "tabId": "tab-1",
            "leafId": RECONCILIATION_LEAF_ID,
            "state": "terminated",
            "createdAt": 1,
            "updatedAt": 2
        }],
        "workspaceSession": serde_json::to_value(terminal_binding_session(Some(
            "ssh:ssh-target@@remote-pty"
        )))
        .unwrap()
    });
    let checkpoint = ShutdownCheckpoint {
        version: 1,
        sessions: vec![ShutdownSession {
            host_id: None,
            state: terminal_binding_session(None),
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();
    let tab = &merged["workspaceSession"]["tabsByWorktree"]["repo-1::D:/workspace"][0];
    let layout = &merged["workspaceSession"]["terminalLayoutsByTabId"]["tab-1"];

    assert!(tab["ptyId"].is_null());
    assert_eq!(layout["ptyIdsByLeafId"], json!({}));
}

#[test]
fn local_checkpoint_rebases_stale_terminal_membership_to_the_host_revision() {
    let mut prior = serde_json::to_value(terminal_binding_session(Some("pty-live"))).unwrap();
    prior["terminalTopologyRevisionByRepoId"] = json!({ "repo-1": 2 });
    let existing = json!({ "ui": {}, "workspaceSession": prior });

    let mut incoming = serde_json::to_value(terminal_binding_session(None)).unwrap();
    incoming["terminalTopologyRevisionByRepoId"] = json!({ "repo-1": 1 });
    incoming["tabsByWorktree"]["repo-1::D:/workspace"] = json!([{
        "id": "stale-tab",
        "ptyId": "pty-stale",
        "worktreeId": "repo-1::D:/workspace",
        "title": "Stale terminal",
        "customTitle": null,
        "color": null,
        "sortOrder": 0,
        "createdAt": 1
    }]);
    incoming["terminalLayoutsByTabId"] = json!({
        "stale-tab": {
            "root": { "type": "leaf", "leafId": RECONCILIATION_LEAF_ID },
            "activeLeafId": RECONCILIATION_LEAF_ID,
            "expandedLeafId": null,
            "ptyIdsByLeafId": { (RECONCILIATION_LEAF_ID): "pty-stale" }
        }
    });
    let checkpoint = ShutdownCheckpoint {
        version: 1,
        sessions: vec![ShutdownSession {
            host_id: None,
            state: serde_json::from_value(incoming).unwrap(),
        }],
        ui: serde_json::from_value(json!({})).unwrap(),
    };

    let merged = apply_shutdown_checkpoint(existing, checkpoint).unwrap();
    let session = &merged["workspaceSession"];

    assert_eq!(
        session["tabsByWorktree"]["repo-1::D:/workspace"][0]["id"],
        json!("tab-1")
    );
    assert_eq!(
        session["terminalLayoutsByTabId"]["tab-1"]["ptyIdsByLeafId"][RECONCILIATION_LEAF_ID],
        json!("pty-live")
    );
    assert!(session["terminalLayoutsByTabId"]["stale-tab"].is_null());
    assert_eq!(
        session["terminalTopologyRevisionByRepoId"]["repo-1"],
        json!(2)
    );
}

#[test]
fn rejects_ui_fields_outside_the_versioned_checkpoint_contract() {
    let checkpoint = serde_json::from_value::<ShutdownCheckpoint>(json!({
        "version": 1,
        "sessions": [],
        "ui": { "sidebarWidth": 320 }
    }));

    assert!(checkpoint.is_err());
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
                state: workspace_session(Some("worktree-1")),
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
        persisted["workspaceSession"]["activeWorktreeId"],
        json!("worktree-1")
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

#[test]
fn settings_service_rejects_oversized_state_before_reading_it() {
    let directory = TestDirectory::new();
    let settings_path = directory.join("mcode-data.json");
    File::create(&settings_path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let service = SettingsService::new(settings_path);

    let result = service.stage_shutdown_checkpoint(ShutdownCheckpoint {
        version: 1,
        sessions: vec![],
        ui: serde_json::from_value(json!({})).unwrap(),
    });

    assert!(matches!(
        result,
        Err(mcode_settings::SettingsServiceError::SettingsTooLarge(_))
    ));
}
