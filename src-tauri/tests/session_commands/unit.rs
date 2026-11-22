use super::*;
use tempfile::TempDir;

fn fixture(document: Value) -> (TempDir, SettingsService) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcode-data.json");
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    (dir, SettingsService::new(path))
}

fn session(active: Option<&str>) -> Value {
    let mut session = default_session();
    session["activeRepoId"] = json!(active);
    session
}

fn terminal_session(pty_id: Option<&str>) -> Value {
    let mut session = session(Some("repo-1"));
    session["tabsByWorktree"] = json!({
        "repo-1::/repo": [{
            "id": "tab-1", "ptyId": pty_id, "worktreeId": "repo-1::/repo",
            "title": "Terminal", "customTitle": null, "color": null,
            "sortOrder": 0, "createdAt": 1
        }]
    });
    session
}

#[test]
fn reads_defaults_without_materializing_missing_host_partitions() {
    let (_dir, service) = fixture(json!({ "settings": { "theme": "dark" } }));
    for host in [None, Some("local"), Some("runtime:peer"), Some("ssh:box")] {
        assert_eq!(session_get_for(&service, host).unwrap(), default_session());
    }
    assert_eq!(
        service.read_persisted_public().unwrap(),
        json!({ "settings": { "theme": "dark" } })
    );
}

#[test]
fn local_and_remote_writes_preserve_sibling_partitions_and_settings() {
    let (_dir, service) = fixture(json!({
        "workspaceSession": session(Some("local-old")),
        "workspaceSessionsByHostId": { "runtime:other": session(Some("other")) },
        "settings": { "theme": "dark" }, "ui": { "sidebarWidth": 321 }
    }));
    session_write_for(&service, session(Some("local-new")), Some(" local "), false).unwrap();
    session_write_for(
        &service,
        session(Some("remote")),
        Some("runtime:peer"),
        false,
    )
    .unwrap();
    let document = service.read_persisted_public().unwrap();
    assert_eq!(document["workspaceSession"]["activeRepoId"], "local-new");
    assert_eq!(
        document["workspaceSessionsByHostId"]["runtime:peer"]["activeRepoId"],
        "remote"
    );
    assert_eq!(
        document["workspaceSessionsByHostId"]["runtime:other"]["activeRepoId"],
        "other"
    );
    assert!(document["workspaceSessionsByHostId"].get("local").is_none());
    assert_eq!(document["settings"]["theme"], "dark");
    assert_eq!(document["ui"]["sidebarWidth"], 321);
}

#[test]
fn patch_replaces_maps_and_keeps_null_as_data() {
    let mut prior = session(Some("repo-1"));
    prior["activeFileIdByWorktree"] = json!({ "a": "file-a", "b": "file-b" });
    prior["lastVisitedAtByWorktreeId"] = json!({ "a": 1 });
    let (_dir, service) = fixture(json!({ "workspaceSession": prior }));
    session_write_for(
        &service,
        json!({
            "activeRepoId": null,
            "activeFileIdByWorktree": { "a": null },
            "lastVisitedAtByWorktreeId": {}
        }),
        None,
        true,
    )
    .unwrap();
    let next = session_get_for(&service, None).unwrap();
    assert!(next.as_object().unwrap().contains_key("activeRepoId"));
    assert_eq!(next["activeRepoId"], Value::Null);
    assert_eq!(next["activeFileIdByWorktree"], json!({ "a": null }));
    assert_eq!(next["lastVisitedAtByWorktreeId"], json!({}));
    assert_eq!(next["browserUrlHistory"], json!([]));
}

#[test]
fn full_set_replaces_optional_fields_instead_of_merging_them() {
    let mut prior = session(Some("old"));
    prior["lastVisitedAtByWorktreeId"] = json!({ "a": 1 });
    let (_dir, service) = fixture(json!({ "workspaceSession": prior }));
    session_write_for(&service, session(None), None, false).unwrap();
    assert!(
        session_get_for(&service, None)
            .unwrap()
            .get("lastVisitedAtByWorktreeId")
            .is_none()
    );
}

#[test]
fn invalid_hosts_and_payloads_fail_without_changing_disk() {
    let initial = json!({ "workspaceSession": session(Some("keep")) });
    let (dir, service) = fixture(initial.clone());
    let path = dir.path().join("mcode-data.json");
    let original = std::fs::read(&path).unwrap();
    for host in ["", "unknown", "runtime:", "ssh:%zz"] {
        assert!(session_get_for(&service, Some(host)).is_err());
        assert!(session_write_for(&service, session(None), Some(host), false).is_err());
    }
    for patch in [
        json!({ "activeRepoId": 12 }),
        json!({ "unexpected": true }),
        json!([]),
    ] {
        assert!(session_write_for(&service, patch, None, true).is_err());
    }
    assert!(session_write_for(&service, json!({ "activeRepoId": null }), None, false).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn malformed_existing_partitions_are_not_silently_replaced_by_defaults() {
    let (_dir, service) = fixture(json!({ "workspaceSession": { "activeRepoId": "broken" } }));
    assert!(session_get_for(&service, None).is_err());
    assert!(session_write_for(&service, json!({ "activeRepoId": null }), None, true).is_err());
}

#[test]
fn set_keeps_live_local_pty_bindings_from_stale_snapshots() {
    let (_dir, service) = fixture(json!({ "workspaceSession": terminal_session(Some("pty-1")) }));
    session_write_for(&service, terminal_session(None), None, false).unwrap();
    let next = session_get_for(&service, None).unwrap();
    assert_eq!(next["tabsByWorktree"]["repo-1::/repo"][0]["ptyId"], "pty-1");
}

#[test]
fn remote_partitions_do_not_copy_pty_authority_from_local_state() {
    let (_dir, service) =
        fixture(json!({ "workspaceSession": terminal_session(Some("local-pty")) }));
    session_write_for(
        &service,
        terminal_session(None),
        Some("runtime:peer"),
        false,
    )
    .unwrap();
    let remote = session_get_for(&service, Some("runtime:peer")).unwrap();
    assert_eq!(
        remote["tabsByWorktree"]["repo-1::/repo"][0]["ptyId"],
        Value::Null
    );
    assert_eq!(
        session_get_for(&service, None).unwrap()["tabsByWorktree"]["repo-1::/repo"][0]["ptyId"],
        "local-pty"
    );
}

#[test]
fn stale_remote_topology_cannot_resurrect_a_removed_terminal() {
    let mut retired = session(None);
    retired["tabsByWorktree"] = json!({ "repo-1::/repo": [] });
    retired["terminalTopologyRevisionByRepoId"] = json!({ "repo-1": 4 });
    let (_dir, service) =
        fixture(json!({ "workspaceSessionsByHostId": { "runtime:peer": retired } }));
    let mut incoming = terminal_session(Some("retired-pty"));
    incoming["terminalTopologyRevisionByRepoId"] = json!({ "repo-1": 3 });
    session_write_for(&service, incoming, Some("runtime:peer"), false).unwrap();
    let next = session_get_for(&service, Some("runtime:peer")).unwrap();
    assert_eq!(next["tabsByWorktree"]["repo-1::/repo"], json!([]));
    assert_eq!(next["terminalTopologyRevisionByRepoId"]["repo-1"], 4);
}

#[test]
fn concurrent_patches_merge_against_current_disk_under_the_service_lock() {
    let (_dir, service) = fixture(json!({ "workspaceSession": session(None) }));
    std::thread::scope(|scope| {
        for index in 0..8 {
            let service = service.clone();
            scope.spawn(move || {
                let host = format!("runtime:peer-{index}");
                session_write_for(&service, json!({ "activeRepoId": host }), Some(&host), true)
                    .unwrap();
            });
        }
    });
    let document = service.read_persisted_public().unwrap();
    assert_eq!(
        document["workspaceSessionsByHostId"]
            .as_object()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(document["workspaceSession"], session(None));
}

#[test]
fn acknowledged_writes_are_readable_from_a_new_service() {
    let (dir, service) = fixture(json!({}));
    session_write_for(&service, session(Some("durable")), None, false).unwrap();
    let reopened = SettingsService::new(dir.path().join("mcode-data.json"));
    assert_eq!(
        session_get_for(&reopened, None).unwrap()["activeRepoId"],
        "durable"
    );
}

#[test]
fn scrollback_reads_only_valid_profile_refs_and_trims_partial_utf8_at_the_tail_boundary() {
    let (dir, service) = fixture(json!({}));
    let root = dir.path().join("terminal-scrollback");
    std::fs::create_dir(&root).unwrap();
    let reference = format!("v1-{}", "a".repeat(32));
    let mut bytes = vec![0xc3, 0xa9];
    bytes.extend(std::iter::repeat_n(
        b'z',
        SCROLLBACK_REPLAY_BYTES as usize - 1,
    ));
    std::fs::write(root.join(format!("{reference}.bin")), bytes).unwrap();
    assert_eq!(
        read_scrollback_for(&service, &reference).unwrap(),
        Some("z".repeat(SCROLLBACK_REPLAY_BYTES as usize - 1))
    );
    for reference in [
        "../../secret",
        "v1-aa",
        &format!("v1-{}", "A".repeat(32)),
        &format!("v1-{}", "b".repeat(32)),
    ] {
        assert_eq!(read_scrollback_for(&service, reference).unwrap(), None);
    }
}

#[test]
fn scrollback_returns_empty_text_for_empty_files_and_reloads_overwritten_refs() {
    let (dir, service) = fixture(json!({}));
    let root = dir.path().join("terminal-scrollback");
    std::fs::create_dir(&root).unwrap();
    let reference = format!("v1-{}", "c".repeat(32));
    let path = root.join(format!("{reference}.bin"));
    std::fs::write(&path, "").unwrap();
    assert_eq!(
        read_scrollback_for(&service, &reference).unwrap(),
        Some(String::new())
    );
    std::fs::write(&path, "restored 👋").unwrap();
    assert_eq!(
        read_scrollback_for(&service, &reference).unwrap(),
        Some("restored 👋".into())
    );
}
