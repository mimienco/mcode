//! agentStatus/agentHooks Tauri commands — Rust port of the invoke handlers in
//! `src/main/ipc/agent-hooks.ts:112-323` (wave 37: snapshot/infer/drop/
//! pane-authority + claude-family install status; other providers report
//! `skipped` honestly until their wave-38 slices land).

use mcode_agent_status::types::{
    AgentHookInstallStatus, AgentInterruptInferenceRequest, AgentQuestionAnsweredInferenceRequest,
    AgentStatusIpcPayload, MigrationUnsupportedPtyEntry,
};

use crate::DesktopCommandError;
use crate::agent_status_http::agent_status_store;

/// Local wrapper over `DesktopCommandError::new` matching the
/// `command_error` helper pattern in pty_commands.rs (no user message, no
/// shutdown intent on this surface).
fn agent_status_error(
    code: &'static str,
    operation: &'static str,
    message: impl Into<String>,
) -> DesktopCommandError {
    DesktopCommandError::new(code, operation, message.into(), "", None, None)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_get_snapshot() -> Result<Vec<AgentStatusIpcPayload>, DesktopCommandError>
{
    let store = agent_status_store();
    let guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_get_snapshot",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    Ok(guard.snapshot_ipc())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_infer_interrupt(
    app: tauri::AppHandle,
    request: AgentInterruptInferenceRequest,
) -> Result<bool, DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_infer_interrupt",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    let now = mcode_agent_status::store::epoch_now();
    let applied = guard.infer_interrupt(&request, now);
    let effect = if applied {
        guard
            .snapshot_ipc()
            .into_iter()
            .find(|row| row.pane_key == request.pane_key)
            .map(mcode_agent_status::store::StoreEffect::Set)
    } else {
        None
    };
    if let Some(effect) = effect {
        crate::agent_status_http::emit_store_effect(&app, &effect);
    }
    Ok(applied)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_infer_question_answered(
    app: tauri::AppHandle,
    request: AgentQuestionAnsweredInferenceRequest,
) -> Result<bool, DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_infer_question_answered",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    let now = mcode_agent_status::store::epoch_now();
    let applied = guard.infer_question_answered(&request, now);
    let effect = if applied {
        guard
            .snapshot_ipc()
            .into_iter()
            .find(|row| row.pane_key == request.pane_key)
            .map(mcode_agent_status::store::StoreEffect::Set)
    } else {
        None
    };
    if let Some(effect) = effect {
        crate::agent_status_http::emit_store_effect(&app, &effect);
    }
    Ok(applied)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_get_migration_unsupported_snapshot()
-> Result<Vec<MigrationUnsupportedPtyEntry>, DesktopCommandError> {
    let store = agent_status_store();
    let guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_get_migration_unsupported_snapshot",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    Ok(guard.migration_unsupported_snapshot())
}

/// Fire-and-forget in the original (`ipcRenderer.send`); under Tauri the
/// renderer awaits the void result, which preserves the dismissal semantics.
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_drop(
    app: tauri::AppHandle,
    pane_key: String,
) -> Result<(), DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_drop",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    let effects = guard.drop_status_entry(&pane_key, mcode_agent_status::store::epoch_now());
    crate::agent_status_http::emit_store_effects(&app, &effects);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_drop_by_tab_prefix(
    app: tauri::AppHandle,
    tab_id: String,
) -> Result<(), DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_drop_by_tab_prefix",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    let effects = guard.drop_status_entries_by_tab_prefix(&tab_id);
    crate::agent_status_http::emit_store_effects(&app, &effects);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_retire_pane_authority(
    pane_key: String,
) -> Result<(), DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_retire_pane_authority",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    guard.retire_pane_authority(&pane_key);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_restore_pane_authority(
    pane_key: String,
) -> Result<bool, DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_restore_pane_authority",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    Ok(guard.restore_pane_authority(&pane_key))
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_status_transfer_pane_authority(
    ptys: tauri::State<'_, crate::pty_commands::NativePtyState>,
    from_pane_key: String,
    to_pane_key: String,
    pty_id: Option<String>,
) -> Result<bool, DesktopCommandError> {
    let store = agent_status_store();
    let mut guard = store.lock().map_err(|error| {
        agent_status_error(
            "agent_status_store_poisoned",
            "agent_status_transfer_pane_authority",
            format!("status store lock poisoned: {error}"),
        )
    })?;
    if from_pane_key == to_pane_key {
        return Ok(false);
    }
    if let Some(pty_id) = pty_id.as_deref()
        && (pty_id.len() > 512 || pty_id.trim().is_empty())
    {
        return Ok(false);
    }
    if !guard.can_transfer_pane_authority(&from_pane_key, pty_id.as_deref(), &|pane, id| {
        ptys.owns_pane(pane, id)
    }) {
        return Ok(false);
    }
    Ok(guard.transfer_pane_authority(&from_pane_key, &to_pane_key, pty_id.as_deref()))
}

/// The 14 hook-install agents (`agent-hook-types.ts:6-21`).
pub(crate) const AGENT_HOOK_TARGETS: [&str; 14] = [
    "claude",
    "openclaude",
    "codex",
    "gemini",
    "antigravity",
    "amp",
    "cursor",
    "droid",
    "command-code",
    "grok",
    "copilot",
    "hermes",
    "devin",
    "kimi",
];

fn home_dir() -> Option<std::path::PathBuf> {
    // Original uses os.homedir(); USERPROFILE/HOME is the equivalent on the
    // platforms this build targets (documented deviation).
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
}

/// `claudeHookService.getStatus()` (`claude/hook-service.ts:145-193`): managed
/// hook entry present in `~/.claude/settings.json`.
fn claude_install_status(agent: &str, settings_dir: &str) -> AgentHookInstallStatus {
    let Some(home) = home_dir() else {
        return AgentHookInstallStatus {
            agent: agent.to_string(),
            state: "error".into(),
            config_path: String::new(),
            managed_hooks_present: false,
            detail: Some("home directory unavailable".into()),
            skip_reason: None,
        };
    };
    let config_path = home.join(settings_dir).join("settings.json");
    let config_path_string = config_path.to_string_lossy().into_owned();
    let parsed: Option<serde_json::Value> = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let Some(parsed) = parsed else {
        return AgentHookInstallStatus {
            agent: agent.to_string(),
            state: "not_installed".into(),
            config_path: config_path_string,
            managed_hooks_present: false,
            detail: None,
            skip_reason: None,
        };
    };
    let managed_present = parsed
        .get("hooks")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|hooks| {
            hooks.values().any(|entries| {
                entries.as_array().is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        entry
                            .get("hooks")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|inner| {
                                inner.iter().any(|hook| {
                                    hook.get("command")
                                        .and_then(serde_json::Value::as_str)
                                        .is_some_and(|command| {
                                            command.contains("mcode") || command.contains("MCode")
                                        })
                                })
                            })
                    })
                })
            })
        });
    AgentHookInstallStatus {
        agent: agent.to_string(),
        state: if managed_present {
            "installed"
        } else {
            "not_installed"
        }
        .into(),
        config_path: config_path_string,
        managed_hooks_present: managed_present,
        detail: None,
        skip_reason: None,
    }
}

/// `agentHooks:<agent>Status` handlers (`ipc/agent-hooks.ts:163-323`).
/// Wave-37 scope: claude/openclaude real file checks; kimi TOML presence;
/// the remaining 11 report `skipped` (honest: the Tauri app has not installed
/// them), never a fabricated `installed`.
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn agent_hooks_status(
    agent: String,
) -> Result<AgentHookInstallStatus, DesktopCommandError> {
    match agent.as_str() {
        "claude" => Ok(claude_install_status("claude", ".claude")),
        "openclaude" => Ok(claude_install_status("openclaude", ".openclaude")),
        "kimi" => {
            let status = home_dir()
                .map(|home| {
                    let config_path = std::env::var_os("KIMI_CODE_HOME")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| home.join(".kimi-code"))
                        .join("config.toml");
                    let managed = std::fs::read_to_string(&config_path)
                        .map(|raw| raw.contains("mcode"))
                        .unwrap_or(false);
                    AgentHookInstallStatus {
                        agent: "kimi".into(),
                        state: if managed {
                            "installed"
                        } else {
                            "not_installed"
                        }
                        .into(),
                        config_path: config_path.to_string_lossy().into_owned(),
                        managed_hooks_present: managed,
                        detail: None,
                        skip_reason: None,
                    }
                })
                .unwrap_or(AgentHookInstallStatus {
                    agent: "kimi".into(),
                    state: "error".into(),
                    config_path: String::new(),
                    managed_hooks_present: false,
                    detail: Some("home directory unavailable".into()),
                    skip_reason: None,
                });
            Ok(status)
        }
        other if AGENT_HOOK_TARGETS.contains(&other) => Ok(AgentHookInstallStatus {
            agent: other.to_string(),
            state: "skipped".into(),
            config_path: String::new(),
            managed_hooks_present: false,
            detail: Some("not yet ported to Tauri (wave 38)".into()),
            skip_reason: None,
        }),
        _ => Err(agent_status_error(
            "agent_hooks_unknown_agent",
            "agent_hooks_status",
            format!("unknown agent hook target: {agent}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_status_is_honest() {
        let status = agent_hooks_status("codex".into()).unwrap();
        assert_eq!(status.state, "skipped");
        assert!(!status.managed_hooks_present);
        assert!(
            status
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("wave 38")
        );
    }

    #[test]
    fn unknown_agent_is_typed_error() {
        assert!(agent_hooks_status("bogus".into()).is_err());
    }

    #[test]
    fn target_list_is_complete() {
        assert_eq!(AGENT_HOOK_TARGETS.len(), 14);
        assert!(AGENT_HOOK_TARGETS.contains(&"command-code"));
    }
}
