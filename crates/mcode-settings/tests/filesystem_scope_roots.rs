use mcode_settings::SettingsService;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "mcode-settings-fs-roots-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn local_filesystem_roots_include_verified_local_scopes_and_exclude_remote_records() {
    let fixture = fixture_path();
    fs::create_dir_all(&fixture).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "repos": [
                {
                    "id": "local-repo",
                    "path": "D:/projects/local",
                    "projectGroupId": "group-local",
                    "importedExternalWorktreePaths": ["C:/Users/secret"]
                },
                {
                    "id": "ssh-repo",
                    "path": "/srv/remote",
                    "connectionId": "ssh-1"
                },
                {
                    "id": "runtime-repo",
                    "path": "/runtime/project",
                    "executionHostId": "runtime:host-1"
                }
            ],
            "folderWorkspaces": [
                {
                    "id": "folder-local",
                    "folderPath": "D:/notes",
                    "projectGroupId": "group-local"
                },
                {
                    "id": "folder-ssh",
                    "folderPath": "/srv/notes",
                    "projectGroupId": "group-ssh",
                    "connectionId": "ssh-1"
                }
            ],
            "projectGroups": [
                {
                    "id": "group-local",
                    "parentPath": "D:/group",
                    "parentGroupId": null
                },
                {
                    "id": "group-ssh",
                    "parentPath": "/srv/group",
                    "connectionId": "ssh-1",
                    "parentGroupId": null
                },
                {
                    "id": "group-runtime",
                    "parentPath": "/runtime/group",
                    "executionHostId": "runtime:host-1",
                    "parentGroupId": null
                }
            ],
            "settings": { "workspaceDir": "../workspaces" },
            "worktreeMeta": {
                "local-repo::C:/Users/secret": {},
                "ssh-repo::/srv/remote-feature": {}
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots()
        .unwrap();

    assert_eq!(
        roots,
        [
            "D:/group",
            "D:/notes",
            "D:/projects/local",
            "D:/projects/workspaces"
        ]
        .map(PathBuf::from)
    );
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn folder_scopes_inherit_remote_groups_and_all_remote_candidate_repos() {
    let fixture = fixture_path();
    fs::create_dir_all(&fixture).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "repos": [{
                "id": "ssh-repo",
                "path": "/srv/team/repo",
                "projectGroupId": "remote-candidates",
                "connectionId": "ssh-1"
            }],
            "projectGroups": [
                {
                    "id": "remote-parent",
                    "parentPath": "/srv/parent",
                    "connectionId": "ssh-1",
                    "parentGroupId": null
                },
                {
                    "id": "remote-candidates",
                    "parentPath": "/srv/team",
                    "parentGroupId": null
                }
            ],
            "folderWorkspaces": [
                {
                    "id": "inherited",
                    "folderPath": "/srv/child/chat",
                    "projectGroupId": "remote-parent"
                },
                {
                    "id": "candidate-owned",
                    "folderPath": "/srv/team",
                    "projectGroupId": "remote-candidates"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots()
        .unwrap();

    assert!(roots.is_empty());
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn wsl_repo_worktree_roots_use_the_distros_actual_home() {
    use std::cell::RefCell;

    let fixture = fixture_path();
    fs::create_dir_all(&fixture).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "repos": [
                {
                    "id": "explicit-linux-base",
                    "path": "\\\\wsl.localhost\\Ubuntu\\home\\jin\\project",
                    "worktreeBasePath": "/home/jin/project/../trees"
                },
                { "id": "different-user", "path": "\\\\wsl$\\Debian\\home\\bob\\repo" },
                { "id": "non-home", "path": "\\\\wsl$\\Debian\\srv\\repo" },
                { "id": "missing-home-one", "path": "\\\\wsl$\\Fedora\\srv\\one" },
                { "id": "missing-home-two", "path": "\\\\wsl$\\Fedora\\opt\\two" }
            ],
            "settings": { "workspaceDir": "D:\\desktop-workspaces" }
        }))
        .unwrap(),
    )
    .unwrap();

    let probe_calls = RefCell::new(Vec::new());
    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots_with_wsl_home(|distro| {
            probe_calls.borrow_mut().push(distro.to_owned());
            (distro == "Debian").then(|| PathBuf::from(r"\\wsl.localhost\Debian\custom\alice"))
        })
        .unwrap();

    assert!(roots.contains(&PathBuf::from(r"\\wsl.localhost\Ubuntu\home\jin\trees")));
    assert!(!roots.contains(&PathBuf::from(
        r"\\wsl.localhost\Debian\home\bob\mcode\workspaces"
    )));
    assert!(roots.contains(&PathBuf::from(
        r"\\wsl.localhost\Debian\custom\alice\mcode\workspaces"
    )));
    assert!(roots.contains(&PathBuf::from(r"D:\desktop-workspaces")));
    assert_eq!(probe_calls.into_inner(), ["Debian", "Fedora"]);
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn global_workspace_root_is_authorized_when_no_local_repo_exists() {
    let fixture = fixture_path();
    let workspace = fixture.join("global-workspaces");
    fs::create_dir_all(&workspace).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "repos": [],
            "settings": { "workspaceDir": workspace.to_string_lossy() }
        }))
        .unwrap(),
    )
    .unwrap();

    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots()
        .unwrap();

    assert_eq!(roots, [workspace]);
    fs::remove_dir_all(fixture).unwrap();
}

#[cfg(windows)]
#[test]
fn wsl_candidate_matching_keeps_the_linux_tail_case_sensitive() {
    let fixture = fixture_path();
    fs::create_dir_all(&fixture).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "repos": [
                { "id": "local-sibling", "path": "\\\\wsl.localhost\\Ubuntu\\home\\Alice\\team\\repo" },
                { "id": "remote-child", "path": "\\\\wsl.localhost\\Ubuntu\\home\\alice\\team\\remote", "connectionId": "ssh-1" }
            ],
            "projectGroups": [{ "id": "folder" }],
            "folderWorkspaces": [{
                "id": "remote-only-folder",
                "folderPath": "\\\\wsl.localhost\\Ubuntu\\home\\alice\\team",
                "projectGroupId": "folder"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots()
        .unwrap();

    assert!(roots.contains(&PathBuf::from(
        r"\\wsl.localhost\Ubuntu\home\Alice\team\repo"
    )));
    assert!(!roots.contains(&PathBuf::from(r"\\wsl.localhost\Ubuntu\home\alice\team")));
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn wsl_home_probe_runs_after_the_settings_lock_is_released() {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let fixture = fixture_path();
    fs::create_dir_all(&fixture).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "repos": [{ "id": "wsl", "path": "\\\\wsl.localhost\\Ubuntu\\srv\\repo" }],
            "settings": { "workspaceDir": "D:\\desktop-workspaces" }
        }))
        .unwrap(),
    )
    .unwrap();
    let service = SettingsService::new(settings_path);
    let writer = service.clone();
    let (done_tx, done_rx) = mpsc::channel();

    let extraction = thread::spawn(move || {
        let result = service.local_filesystem_roots_with_wsl_home(|_| {
            writer.set_minimize_to_tray_on_close(true).unwrap();
            None
        });
        done_tx.send(result).unwrap();
    });

    assert!(
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .is_ok()
    );
    extraction.join().unwrap();
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn root_extraction_retries_when_settings_change_during_a_wsl_probe() {
    let fixture = fixture_path();
    let replacement = fixture.join("replacement");
    fs::create_dir_all(&replacement).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        br#"{"repos":[{"id":"old","path":"\\\\wsl.localhost\\Ubuntu\\srv\\repo"}],"settings":{"workspaceDir":"D:\\desktop-workspaces"}}"#,
    )
    .unwrap();
    let replacement_payload = serde_json::to_vec(&json!({
        "repos": [{ "id": "replacement", "path": replacement.to_string_lossy() }]
    }))
    .unwrap();
    let path_during_probe = settings_path.clone();

    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots_with_wsl_home(move |_| {
            fs::write(&path_during_probe, &replacement_payload).unwrap();
            Some(PathBuf::from(r"\\wsl.localhost\Ubuntu\custom\stale"))
        })
        .unwrap();

    assert_eq!(roots, [replacement]);
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn local_filesystem_roots_fail_closed_for_malformed_or_missing_collections() {
    let fixture = fixture_path();
    fs::create_dir_all(&fixture).unwrap();
    let settings_path = fixture.join("mcode-data.json");
    fs::write(
        &settings_path,
        br#"{"repos":"invalid","folderWorkspaces":null,"projectGroups":[{"parentPath":""}]}"#,
    )
    .unwrap();

    let roots = SettingsService::new(settings_path)
        .local_filesystem_roots()
        .unwrap();

    assert!(roots.is_empty());
    fs::remove_dir_all(fixture).unwrap();
}
