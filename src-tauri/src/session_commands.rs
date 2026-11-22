//! Async preload session contract over the durable settings service.
//! Patch replaces top-level slices; checkpoint reconciliation owns PTY fences.

use mcode_settings::{
    ExecutionHostId, SettingsService, SettingsServiceError, ShutdownCheckpoint, ShutdownSession,
    WorkspaceSessionState, apply_shutdown_checkpoint,
};
use serde_json::{Map, Value, json};
use std::io::{Read, Seek, SeekFrom};

use crate::DesktopCommandError;

fn session_host(host_id: Option<&str>) -> Result<Option<ExecutionHostId>, SettingsServiceError> {
    let Some(host_id) = host_id else {
        return Ok(None);
    };
    let host = ExecutionHostId::parse(host_id).map_err(SettingsServiceError::Parse)?;
    Ok((host_id.trim() != "local").then_some(host))
}

fn default_session() -> Value {
    json!({
        "activeRepoId": null,
        "activeWorktreeId": null,
        "activeTabId": null,
        "tabsByWorktree": {},
        "terminalLayoutsByTabId": {},
        "openFilesByWorktree": {},
        "markdownFrontmatterVisible": {},
        "browserTabsByWorktree": {},
        "browserPagesByWorkspace": {},
        "activeBrowserTabIdByWorktree": {},
        "activeFileIdByWorktree": {},
        "activeTabTypeByWorktree": {},
        "browserUrlHistory": [],
        "defaultTerminalTabsAppliedByWorktreeId": {}
    })
}

fn partition(
    root: &Map<String, Value>,
    host: Option<&ExecutionHostId>,
) -> Result<Value, SettingsServiceError> {
    let stored = if let Some(host) = host {
        let key = serde_json::to_value(host)
            .map_err(|error| SettingsServiceError::Serialize(error.to_string()))?;
        let partitions = root
            .get("workspaceSessionsByHostId")
            .map(|value| {
                value
                    .as_object()
                    .ok_or(SettingsServiceError::PersistedSectionMustBeObject(
                        "workspaceSessionsByHostId",
                    ))
            })
            .transpose()?;
        partitions.and_then(|partitions| partitions.get(key.as_str().unwrap_or_default()))
    } else {
        root.get("workspaceSession")
    };
    let value = stored.cloned().unwrap_or_else(default_session);
    validate_session(value.clone())?;
    Ok(value)
}

fn validate_session(value: Value) -> Result<WorkspaceSessionState, SettingsServiceError> {
    serde_json::from_value(value).map_err(|error| SettingsServiceError::Parse(error.to_string()))
}

fn session_get_for(
    service: &SettingsService,
    host_id: Option<&str>,
) -> Result<Value, SettingsServiceError> {
    let host = session_host(host_id)?;
    let document = service.read_persisted_public()?;
    let root = document
        .as_object()
        .ok_or(SettingsServiceError::PersistedSectionMustBeObject("root"))?;
    partition(root, host.as_ref())
}

fn session_write_for(
    service: &SettingsService,
    value: Value,
    host_id: Option<&str>,
    patch: bool,
) -> Result<(), SettingsServiceError> {
    let host = session_host(host_id)?;
    let fields = value
        .as_object()
        .ok_or_else(|| SettingsServiceError::Parse("session write must be an object".into()))?;
    service.update_persisted_public(|root| {
        let next = if patch {
            let mut prior = partition(root, host.as_ref())?;
            let prior_fields = prior.as_object_mut().ok_or_else(|| {
                SettingsServiceError::Parse("session partition must be an object".into())
            })?;
            // Electron's patchWorkspaceSession uses {...current, ...patch}, including null.
            prior_fields.extend(fields.clone());
            prior
        } else {
            value.clone()
        };
        let checkpoint = ShutdownCheckpoint {
            version: mcode_settings::SHUTDOWN_CHECKPOINT_VERSION,
            sessions: vec![ShutdownSession {
                state: validate_session(next)?,
                host_id: host,
            }],
            ui: Default::default(),
        };
        let reconciled = apply_shutdown_checkpoint(Value::Object(root.clone()), checkpoint)
            .map_err(SettingsServiceError::Checkpoint)?;
        *root = reconciled
            .as_object()
            .cloned()
            .ok_or(SettingsServiceError::PersistedSectionMustBeObject("root"))?;
        Ok(())
    })
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn session_get(
    service: tauri::State<'_, SettingsService>,
    host_id: Option<String>,
) -> Result<Value, DesktopCommandError> {
    session_get_for(&service, host_id.as_deref())
        .map_err(|error| DesktopCommandError::settings(error, "session_get"))
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn session_set(
    service: tauri::State<'_, SettingsService>,
    args: Value,
    host_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    session_write_for(&service, args, host_id.as_deref(), false)
        .map_err(|error| DesktopCommandError::settings(error, "session_set"))
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn session_patch(
    service: tauri::State<'_, SettingsService>,
    args: Value,
    host_id: Option<String>,
) -> Result<(), DesktopCommandError> {
    session_write_for(&service, args, host_id.as_deref(), true)
        .map_err(|error| DesktopCommandError::settings(error, "session_patch"))
}

#[tauri::command]
pub(crate) fn session_flush(
    service: tauri::State<'_, SettingsService>,
) -> Result<(), DesktopCommandError> {
    // Writes already sync and atomically commit before responding; the client drains its queue.
    service
        .read_persisted_public()
        .map(|_| ())
        .map_err(|error| DesktopCommandError::settings(error, "session_flush"))
}

const SCROLLBACK_REPLAY_BYTES: u64 = 512 * 1024;

fn read_scrollback_for(
    service: &SettingsService,
    reference: &str,
) -> Result<Option<String>, SettingsServiceError> {
    if reference.len() != 35
        || !reference.starts_with("v1-")
        || !reference.as_bytes()[3..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Ok(None);
    }
    let parent = service
        .path()
        .parent()
        .ok_or_else(|| SettingsServiceError::Read("settings path has no parent".into()))?;
    let root = match parent.join("terminal-scrollback").canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SettingsServiceError::Read(error.to_string())),
    };
    let path = match root.join(format!("{reference}.bin")).canonicalize() {
        Ok(path) if path.starts_with(&root) => path,
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SettingsServiceError::Read(error.to_string())),
    };
    let payload = (|| -> std::io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(std::io::Error::other(
                "scrollback snapshot is not a regular file",
            ));
        }
        let count = metadata.len().min(SCROLLBACK_REPLAY_BYTES);
        file.seek(SeekFrom::Start(metadata.len() - count))?;
        let mut bytes = Vec::with_capacity(count as usize);
        file.take(count).read_to_end(&mut bytes)?;
        let start = bytes
            .iter()
            .position(|byte| byte & 0xc0 != 0x80)
            .unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[start..]).into_owned())
    })()
    .map_err(|error| SettingsServiceError::Read(error.to_string()))?;
    Ok(Some(payload))
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn session_read_terminal_scrollback(
    service: tauri::State<'_, SettingsService>,
    reference: String,
) -> Result<Option<String>, DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || read_scrollback_for(&service, &reference))
        .await
        .map_err(|error| {
            DesktopCommandError::settings(
                SettingsServiceError::Read(error.to_string()),
                "session_read_terminal_scrollback",
            )
        })?
        .map_err(|error| DesktopCommandError::settings(error, "session_read_terminal_scrollback"))
}

#[cfg(test)]
#[path = "../tests/session_commands/unit.rs"]
mod tests;
