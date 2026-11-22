//! Local catalog and execution-host-owned workspace discovery.
use std::path::Path;

use mcode_git::{Repo, WorktreeEntry, WorktreeService};
use mcode_settings::SettingsService;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::DesktopCommandError;
use crate::workspace_commands::{
    WorkspacePathScopeHandle, command_error, folder_workspace_comparator, is_local_record,
    path_comparison_key, settings_error, strip_verbatim_prefix,
};

const CATALOG: &str = "workspace_catalog_get";
const DETECTED: &str = "workspace_list_detected";
const PATH_STATUS: &str = "workspace_path_status";
const PICK: &str = "workspace_pick_folder";

fn read_document(
    service: &SettingsService,
    operation: &'static str,
) -> Result<Value, DesktopCommandError> {
    service
        .read_persisted_public()
        .map_err(|error| settings_error(error, operation))
}

fn records<'a>(document: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    document
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn local(value: &Value) -> bool {
    value.as_object().is_some_and(is_local_record)
}

fn remote_group(document: &Value, group_id: Option<&str>) -> bool {
    group_id.is_some_and(|id| {
        records(document, "projectGroups")
            .any(|group| group.get("id").and_then(Value::as_str) == Some(id) && !local(group))
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceCatalog {
    repos: Vec<Value>,
    project_groups: Vec<Value>,
    folder_workspaces: Vec<Value>,
}

fn catalog_owned(row: &Value, require_connection_for_remote: bool) -> bool {
    let Some(row) = row.as_object() else {
        return false;
    };
    match row.get("executionHostId").and_then(Value::as_str) {
        None | Some("local") => true,
        Some(host) if host.starts_with("ssh:") => {
            !require_connection_for_remote
                || row
                    .get("connectionId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty())
        }
        Some(_) => false,
    }
}

pub(crate) fn workspace_catalog_get_for(
    service: &SettingsService,
) -> Result<WorkspaceCatalog, DesktopCommandError> {
    let document = read_document(service, CATALOG)?;
    // This client owns local and direct-SSH catalog metadata; scans still require the owning host.
    let project_groups: Vec<Value> = records(&document, "projectGroups")
        .filter(|row| catalog_owned(row, true))
        .cloned()
        .collect();
    let mut folder_workspaces: Vec<_> = records(&document, "folderWorkspaces")
        .filter(|row| catalog_owned(row, true))
        .filter(|row| {
            row.get("projectGroupId")
                .and_then(Value::as_str)
                .is_none_or(|group_id| {
                    !records(&document, "projectGroups").any(|group| {
                        group.get("id").and_then(Value::as_str) == Some(group_id)
                            && !catalog_owned(group, true)
                    })
                })
        })
        .cloned()
        .collect();
    folder_workspaces.sort_by(folder_workspace_comparator());
    Ok(WorkspaceCatalog {
        repos: records(&document, "repos")
            .filter(|row| catalog_owned(row, false))
            .cloned()
            .collect(),
        project_groups,
        folder_workspaces,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListDetectedArgs {
    provider_request_id: String,
    repo_id: String,
    execution_host_id: String,
}

fn rejected(args: &ListDetectedArgs, status: &str) -> Value {
    json!({"status": status, "providerRequestId": args.provider_request_id,
        "executionHostId": args.execution_host_id})
}

fn stored_repo<'a>(document: &'a Value, id: &str) -> Option<&'a Value> {
    records(document, "repos").find(|row| row.get("id").and_then(Value::as_str) == Some(id))
}

fn local_metadata(meta: &Value) -> bool {
    local(meta)
        && meta
            .get("hostId")
            .is_none_or(|host| host.is_null() || host.as_str() == Some("local"))
}

fn metadata_for_path<'a>(
    metadata: Option<&'a Map<String, Value>>,
    repo_id: &str,
    path: &str,
) -> Result<(String, Option<&'a Value>), ()> {
    let prefix = format!("{repo_id}::");
    let expected = path_comparison_key(path);
    let matches: Vec<_> = metadata
        .into_iter()
        .flat_map(|metadata| metadata.iter())
        .filter(|(id, _)| {
            id.strip_prefix(&prefix)
                .is_some_and(|path| path_comparison_key(path) == expected)
        })
        .collect();
    // Preserve a unique persisted ID so slash/case normalization cannot detach session metadata.
    match matches.as_slice() {
        [] => Ok((format!("{prefix}{path}"), None)),
        [(id, meta)] if local_metadata(meta) => Ok(((*id).clone(), Some(*meta))),
        _ => Err(()),
    }
}

fn projected_worktree(
    repo: &Value,
    entry: &WorktreeEntry,
    id: &str,
    meta: Option<&Value>,
    is_main: bool,
) -> Value {
    // Keep optional provenance only when persisted; fabricated creation fields change ownership policy.
    let mut row = meta.and_then(Value::as_object).cloned().unwrap_or_default();
    for key in [
        "linkedIssue",
        "linkedPR",
        "linkedLinearIssue",
        "linkedLinearIssueWorkspaceId",
        "linkedLinearIssueOrganizationUrlKey",
        "linkedGitLabMR",
        "linkedGitLabIssue",
        "linkedBitbucketPR",
        "linkedAzureDevOpsPR",
        "linkedGiteaPR",
        "linkedWorkItem",
        "linkedTaskSourceContext",
    ] {
        row.entry(key).or_insert(Value::Null);
    }
    for key in ["isArchived", "isUnread", "isPinned"] {
        row.entry(key).or_insert(Value::Bool(false));
    }
    for key in ["sortOrder", "lastActivityAt"] {
        row.entry(key).or_insert(json!(0));
    }
    row.entry("comment").or_insert(json!(""));
    row.entry("workspaceStatus").or_insert(json!("in-progress"));
    let repo_name = repo
        .get("displayName")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty());
    let fallback_name = if repo.get("kind").and_then(Value::as_str) == Some("folder") {
        repo_name.unwrap_or("").to_owned()
    } else {
        entry
            .branch
            .as_deref()
            .map(|branch| branch.strip_prefix("refs/heads/").unwrap_or(branch))
            .filter(|branch| !branch.is_empty())
            .or(repo_name)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                Path::new(&entry.path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
    };
    if row
        .get("displayName")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        row.insert("displayName".into(), json!(fallback_name));
    }
    row.insert("id".into(), json!(id));
    row.insert("repoId".into(), repo["id"].clone());
    row.insert("path".into(), json!(entry.path));
    row.insert("head".into(), json!(entry.head.as_deref().unwrap_or("")));
    row.insert(
        "branch".into(),
        json!(entry.branch.as_deref().unwrap_or("")),
    );
    row.insert("isBare".into(), json!(entry.bare));
    row.insert("isMainWorktree".into(), json!(is_main));
    row.insert("hostId".into(), json!("local"));
    for key in ["locked", "lockReason", "prunable", "prunableReason"] {
        row.remove(key);
    }
    if let Some(reason) = &entry.locked_reason {
        row.insert("locked".into(), json!(true));
        row.insert("lockReason".into(), json!(reason));
    }
    // The renderer applies the existing shared ownership/visibility policy to these observations.
    let selected = repo
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| path_comparison_key(path) == path_comparison_key(&entry.path));
    row.insert("selectedCheckout".into(), json!(selected));
    row.insert("ownership".into(), json!("unknown-legacy"));
    row.insert("visible".into(), json!(selected));
    Value::Object(row)
}

pub(crate) fn workspace_list_detected_for(
    service: &SettingsService,
    args: ListDetectedArgs,
) -> Result<Value, DesktopCommandError> {
    if args.execution_host_id != "local"
        || args.provider_request_id.trim().is_empty()
        || args.provider_request_id.len() > 128
        || args.provider_request_id.chars().any(char::is_control)
        || args.repo_id.trim().is_empty()
    {
        return Ok(rejected(&args, "rejected"));
    }
    let document = read_document(service, DETECTED)?;
    let Some(repo) = stored_repo(&document, &args.repo_id).filter(|repo| local(repo)) else {
        return Ok(rejected(&args, "rejected"));
    };
    let path = repo
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            command_error("workspace_repo_path_invalid", DETECTED, "repo has no path")
        })?;
    if !Path::new(path).is_absolute() || !Path::new(path).is_dir() {
        return Err(command_error(
            "workspace_open_path_unusable",
            DETECTED,
            "repo path is not an existing absolute directory",
        ));
    }
    let folder = repo.get("kind").and_then(Value::as_str) == Some("folder");
    let entries = if folder {
        vec![WorktreeEntry {
            path: path.to_owned(),
            ..Default::default()
        }]
    } else {
        let discovered = Repo::discover(Path::new(path))
            .map_err(|error| command_error(error.code(), DETECTED, error.to_string()))?;
        WorktreeService::new(&discovered)
            .list()
            .map_err(|error| command_error(error.code(), DETECTED, error.to_string()))?
    };
    let fresh = read_document(service, DETECTED)?;
    let Some(current) = stored_repo(&fresh, &args.repo_id) else {
        return Ok(rejected(&args, "stale"));
    };
    if !local(current)
        || current.get("path") != repo.get("path")
        || current.get("kind") != repo.get("kind")
    {
        return Ok(rejected(&args, "stale"));
    }
    let metadata = fresh.get("worktreeMeta").and_then(Value::as_object);
    let mut worktrees = Vec::new();
    if folder {
        let Ok((root_id, _)) = metadata_for_path(metadata, &args.repo_id, path) else {
            return Ok(rejected(&args, "ambiguous-owner"));
        };
        let prefix = format!("{}::", args.repo_id);
        let expected_path = path_comparison_key(path);
        let mut ids = vec![root_id.clone()];
        if let Some(metadata) = metadata {
            ids.extend(
                metadata
                    .keys()
                    .filter(|id| {
                        id.strip_prefix(&prefix)
                            .and_then(|suffix| suffix.rsplit_once("::workspace:"))
                            .is_some_and(|(root_path, instance)| {
                                !instance.is_empty()
                                    && path_comparison_key(root_path) == expected_path
                            })
                    })
                    .cloned(),
            );
        }
        for id in ids {
            let meta = metadata.and_then(|metadata| metadata.get(&id));
            if meta.is_some_and(|meta| !local_metadata(meta)) {
                return Ok(rejected(&args, "ambiguous-owner"));
            }
            worktrees.push(projected_worktree(
                current,
                &entries[0],
                &id,
                meta,
                id == root_id,
            ));
        }
        worktrees.sort_by(|a, b| {
            let main_a = a["isMainWorktree"].as_bool().unwrap_or(false);
            let main_b = b["isMainWorktree"].as_bool().unwrap_or(false);
            main_b.cmp(&main_a).then_with(|| {
                let time = |row: &Value| {
                    row.get("createdAt")
                        .or_else(|| row.get("lastActivityAt"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                };
                time(b).cmp(&time(a))
            })
        });
    } else {
        for (index, entry) in entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.prunable_reason.is_none())
        {
            let Ok((id, meta)) = metadata_for_path(metadata, &args.repo_id, &entry.path) else {
                return Ok(rejected(&args, "ambiguous-owner"));
            };
            worktrees.push(projected_worktree(current, entry, &id, meta, index == 0));
        }
    }
    Ok(
        json!({"status":"complete", "providerRequestId":args.provider_request_id,
        "repoId":args.repo_id, "authority":{"kind":"local","executionHostId":"local"},
        "result":{"repoId":args.repo_id,"authoritative":true,"source":"git","worktrees":worktrees}}),
    )
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "scope", rename_all = "kebab-case")]
pub(crate) enum PathStatusArgs {
    #[serde(rename_all = "camelCase")]
    FolderWorkspace { folder_workspace_id: String },
    #[serde(rename_all = "camelCase")]
    ProjectGroup { project_group_id: String },
    #[serde(rename_all = "camelCase")]
    Path {
        path: String,
        connection_id: Option<String>,
        execution_host_id: Option<String>,
    },
}

pub(crate) fn workspace_path_status_for(
    service: &SettingsService,
    args: PathStatusArgs,
) -> Result<Value, DesktopCommandError> {
    let document = read_document(service, PATH_STATUS)?;
    let (path, remote) = match args {
        PathStatusArgs::Path {
            path,
            connection_id,
            execution_host_id,
        } => (
            path,
            connection_id.is_some() || execution_host_id.is_some_and(|host| host != "local"),
        ),
        PathStatusArgs::FolderWorkspace {
            folder_workspace_id,
        } => {
            let row = records(&document, "folderWorkspaces")
                .find(|row| row.get("id").and_then(Value::as_str) == Some(&folder_workspace_id))
                .ok_or_else(|| {
                    command_error(
                        "workspace_record_missing",
                        PATH_STATUS,
                        "folder workspace is not in the catalog",
                    )
                })?;
            (
                row.get("folderPath")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                !local(row)
                    || remote_group(&document, row.get("projectGroupId").and_then(Value::as_str)),
            )
        }
        PathStatusArgs::ProjectGroup { project_group_id } => {
            let row = records(&document, "projectGroups")
                .find(|row| row.get("id").and_then(Value::as_str) == Some(&project_group_id))
                .ok_or_else(|| {
                    command_error(
                        "workspace_record_missing",
                        PATH_STATUS,
                        "project group is not in the catalog",
                    )
                })?;
            (
                row.get("parentPath")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                !local(row),
            )
        }
    };
    if remote || !Path::new(&path).is_absolute() {
        return Ok(json!({"path":path,"exists":false,"reason":"unavailable"}));
    }
    Ok(match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => json!({"path":path,"exists":true}),
        Ok(_) => json!({"path":path,"exists":false,"reason":"not-directory"}),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            json!({"path":path,"exists":false,"reason":"missing"})
        }
        Err(_) => json!({"path":path,"exists":false,"reason":"unavailable"}),
    })
}

#[tauri::command]
pub(crate) async fn workspace_catalog_get(
    service: tauri::State<'_, SettingsService>,
) -> Result<WorkspaceCatalog, DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspace_catalog_get_for(&service))
        .await
        .map_err(|error| {
            command_error(
                "workspace_catalog_get_join_failed",
                CATALOG,
                error.to_string(),
            )
        })?
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn workspace_list_detected(
    service: tauri::State<'_, SettingsService>,
    args: ListDetectedArgs,
) -> Result<Value, DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspace_list_detected_for(&service, args))
        .await
        .map_err(|error| {
            command_error(
                "workspace_list_detected_join_failed",
                DETECTED,
                error.to_string(),
            )
        })?
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn workspace_path_status(
    service: tauri::State<'_, SettingsService>,
    args: PathStatusArgs,
) -> Result<Value, DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspace_path_status_for(&service, args))
        .await
        .map_err(|error| {
            command_error(
                "workspace_path_status_join_failed",
                PATH_STATUS,
                error.to_string(),
            )
        })?
}

#[tauri::command]
pub(crate) async fn workspace_pick_folder(
    app: tauri::AppHandle,
    scope: tauri::State<'_, WorkspacePathScopeHandle>,
) -> Result<Option<String>, DesktopCommandError> {
    let scope = scope.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let selected = crate::app_dialogs::pick_floating_workspace_directory(&app)
            .map_err(|error| command_error("workspace_pick_folder_failed", PICK, error))?;
        workspace_picked_folder_for(&scope, selected)
    })
    .await
    .map_err(|error| command_error("workspace_pick_folder_join_failed", PICK, error.to_string()))?
}

pub(crate) fn workspace_picked_folder_for(
    scope: &WorkspacePathScopeHandle,
    selected: Option<String>,
) -> Result<Option<String>, DesktopCommandError> {
    selected
        .map(|path| {
            scope
                .register_native_picked_directory(Path::new(&path))
                .map(|canonical| {
                    strip_verbatim_prefix(&canonical)
                        .to_string_lossy()
                        .into_owned()
                })
        })
        .transpose()
}

#[cfg(test)]
#[path = "workspace_catalog_commands_tests.rs"]
mod tests;
