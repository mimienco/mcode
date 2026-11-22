use super::*;
use tempfile::TempDir;

fn service(root: &Path, document: Value) -> SettingsService {
    let path = root.join("settings.json");
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    SettingsService::new(path)
}

fn args(repo_id: &str) -> ListDetectedArgs {
    ListDetectedArgs {
        provider_request_id: "scan-1".into(),
        repo_id: repo_id.into(),
        execution_host_id: "local".into(),
    }
}

fn git(root: &Path, args: &[&str]) {
    let mut command = std::process::Command::new("git");
    command.current_dir(root).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().expect("git available");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn catalog_preserves_local_and_ssh_metadata_without_importing_runtime_catalogs() {
    let dir = TempDir::new().unwrap();
    let service = service(
        dir.path(),
        json!({
            "repos":[{"id":"folder-repo","kind":"folder"},{"id":"git-repo"},
                     {"id":"remote","executionHostId":"ssh:box"},
                 {"id":"runtime","executionHostId":"runtime:box"}],
            "projectGroups":[{"id":"local-group"},{"id":"remote-group","connectionId":"box"},
                         {"id":"runtime-group","executionHostId":"runtime:box"}],
            "folderWorkspaces":[{"id":"one","projectGroupId":"local-group","sortOrder":1},
                {"id":"two","projectGroupId":"local-group","sortOrder":2},
                {"id":"inherited","projectGroupId":"remote-group"},
                {"id":"remote","connectionId":"box"},
            {"id":"runtime-child","projectGroupId":"runtime-group"}]
        }),
    );
    let before = std::fs::read(dir.path().join("settings.json")).unwrap();
    let catalog = workspace_catalog_get_for(&service).unwrap();
    assert_eq!(catalog.repos.len(), 3);
    assert_eq!(catalog.repos[0]["kind"], "folder");
    assert_eq!(catalog.project_groups.len(), 2);
    assert_eq!(catalog.folder_workspaces.len(), 4);
    assert_eq!(catalog.folder_workspaces[0]["id"], "two");
    assert_eq!(
        std::fs::read(dir.path().join("settings.json")).unwrap(),
        before
    );
}

#[test]
fn folder_scan_keeps_instances_and_saved_metadata_without_invented_creation_provenance() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_string_lossy().into_owned();
    let root_id = format!("folder::{path}");
    let instance_id = format!("{root_id}::workspace:one");
    let service = service(
        dir.path(),
        json!({
            "repos":[{"id":"folder","path":path,"kind":"folder","displayName":"Folder"}],
            "worktreeMeta":{instance_id.clone():{"displayName":"Saved instance","createdAt":15,"comment":"keep","isPinned":true}}
        }),
    );
    let result = workspace_list_detected_for(&service, args("folder")).unwrap();
    assert_eq!(result["status"], "complete");
    assert_eq!(result["result"]["authoritative"], true);
    let rows = result["result"]["worktrees"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], root_id);
    assert_eq!(rows[0]["isMainWorktree"], true);
    assert!(rows[0].get("createdAt").is_none());
    assert_eq!(rows[1]["id"], instance_id);
    assert_eq!(rows[1]["displayName"], "Saved instance");
    assert_eq!(rows[1]["createdAt"], 15);
    assert_eq!(rows[1]["comment"], "keep");
    assert_eq!(rows[1]["isPinned"], true);
}

#[test]
fn git_scan_enumerates_real_linked_checkout_and_preserves_saved_metadata() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main");
    let linked = dir.path().join("linked checkout");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init"]);
    git(
        &main,
        &[
            "-c",
            "user.name=Migration Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ],
    );
    git(
        &main,
        &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
    );
    let git_repo = Repo::discover(&main).unwrap();
    let entries = WorktreeService::new(&git_repo).list().unwrap();
    let linked_entry = entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("refs/heads/feature"))
        .unwrap();
    let linked_id = format!("git::{}", linked_entry.path);
    let service = service(
        dir.path(),
        json!({
            "repos":[{"id":"git","path":main,"displayName":"Main","kind":"git"}],
            "worktreeMeta":{linked_id.clone():{"displayName":"Pinned task","isPinned":true,"createdAt":12,"head":"stale","path":"forged"}}
        }),
    );
    let result = workspace_list_detected_for(&service, args("git")).unwrap();
    let rows = result["result"]["worktrees"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["isMainWorktree"], true);
    let row = rows.iter().find(|row| row["id"] == linked_id).unwrap();
    assert_eq!(row["branch"], "refs/heads/feature");
    assert_eq!(row["path"], linked_entry.path);
    assert_eq!(row["head"], linked_entry.head.as_deref().unwrap());
    assert_eq!(row["displayName"], "Pinned task");
    assert_eq!(row["isPinned"], true);
    assert_eq!(row["createdAt"], 12);
    assert_eq!(row["isMainWorktree"], false);
}

#[test]
fn git_failure_never_becomes_an_authoritative_empty_or_synthetic_scan() {
    let dir = TempDir::new().unwrap();
    let service = service(
        dir.path(),
        json!({"repos":[{"id":"git","path":dir.path(),"kind":"git"}]}),
    );
    let error = workspace_list_detected_for(&service, args("git")).unwrap_err();
    assert_eq!(error.code, "git_not_a_repository");
}

#[test]
fn remote_scan_and_conflicting_persisted_owner_are_rejected() {
    let dir = TempDir::new().unwrap();
    let root_id = format!("local::{}", dir.path().display());
    let service = service(
        dir.path(),
        json!({
            "repos":[{"id":"remote","path":"/unreachable","connectionId":"box"},
                      {"id":"local","path":dir.path(),"kind":"folder"}],
            "worktreeMeta":{root_id:{"hostId":"ssh:box"}}
        }),
    );
    assert_eq!(
        workspace_list_detected_for(&service, args("remote")).unwrap()["status"],
        "rejected"
    );
    let mut remote = args("local");
    remote.execution_host_id = "ssh:box".into();
    assert_eq!(
        workspace_list_detected_for(&service, remote).unwrap()["status"],
        "rejected"
    );
    assert_eq!(
        workspace_list_detected_for(&service, args("local")).unwrap()["status"],
        "ambiguous-owner"
    );
    let mut invalid = args("local");
    invalid.provider_request_id = "x".repeat(129);
    assert_eq!(
        workspace_list_detected_for(&service, invalid).unwrap()["status"],
        "rejected"
    );
}

#[test]
fn path_status_distinguishes_local_missing_files_and_unavailable_remote_paths() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.txt");
    std::fs::write(&file, "text").unwrap();
    let missing = dir.path().join("missing");
    let service = service(
        dir.path(),
        json!({
            "projectGroups":[{"id":"remote","parentPath":missing,"connectionId":"box"}],
            "folderWorkspaces":[{"id":"remote-folder","folderPath":missing,"projectGroupId":"remote"}]
        }),
    );
    for (path, reason) in [
        (dir.path(), None),
        (file.as_path(), Some("not-directory")),
        (missing.as_path(), Some("missing")),
    ] {
        let result = workspace_path_status_for(
            &service,
            PathStatusArgs::Path {
                path: path.to_string_lossy().into_owned(),
                connection_id: None,
                execution_host_id: None,
            },
        )
        .unwrap();
        assert_eq!(result["exists"], reason.is_none());
        assert_eq!(result.get("reason").and_then(Value::as_str), reason);
    }
    for request in [
        PathStatusArgs::ProjectGroup {
            project_group_id: "remote".into(),
        },
        PathStatusArgs::FolderWorkspace {
            folder_workspace_id: "remote-folder".into(),
        },
        PathStatusArgs::Path {
            path: missing.to_string_lossy().into_owned(),
            connection_id: Some("box".into()),
            execution_host_id: None,
        },
    ] {
        assert_eq!(
            workspace_path_status_for(&service, request).unwrap()["reason"],
            "unavailable"
        );
    }
}

#[test]
fn windows_git_path_variants_reattach_unique_saved_id_and_metadata() {
    let saved_id = r"repo::C:\Work\My Project";
    let metadata = json!({saved_id:{"displayName":"Saved title","isPinned":true,"createdAt":42,"creatorProvenance":{"kind":"host"}}});
    let (id, meta) = metadata_for_path(metadata.as_object(), "repo", "c:/work/my project").unwrap();
    assert_eq!(id, saved_id);
    let row = projected_worktree(
        &json!({"id":"repo","path":r"C:\Work\My Project"}),
        &WorktreeEntry {
            path: "c:/work/my project".into(),
            ..Default::default()
        },
        &id,
        meta,
        true,
    );
    assert_eq!(row["displayName"], "Saved title");
    assert_eq!(row["isPinned"], true);
    assert_eq!(row["createdAt"], 42);
    assert_eq!(row["creatorProvenance"]["kind"], "host");
    assert_eq!(row["selectedCheckout"], true);
    let ambiguous =
        json!({saved_id:{"displayName":"local"},"repo::c:/work/my project":{"hostId":"ssh:box"}});
    assert!(metadata_for_path(ambiguous.as_object(), "repo", "c:/work/my project").is_err());
}
