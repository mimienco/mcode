use super::is_execution_host_id;
use serde_json::{Map, Value};

pub(super) fn validate_workspace_session_state(fields: &Map<String, Value>) -> Result<(), String> {
    for required in [
        "activeRepoId",
        "activeWorktreeId",
        "activeTabId",
        "tabsByWorktree",
        "terminalLayoutsByTabId",
    ] {
        if !fields.contains_key(required) {
            return Err(format!("workspace session is missing {required}"));
        }
    }

    for (name, value) in fields {
        let valid = match name.as_str() {
            "activeRepoId" | "activeWorktreeId" | "activeTabId" => is_nullable_string(value),
            "activeWorkspaceKey" => value.is_null() || value.as_str().is_some_and(is_workspace_key),
            "activeWorkspaceExecutionHostId" => {
                value.is_null() || value.as_str().is_some_and(is_execution_host_id)
            }
            "tabsByWorktree" => {
                is_record_of(value, |entry| is_array_of(entry, validate_terminal_tab))
            }
            "terminalLayoutsByTabId" => is_record_of_with_key(value, |tab_id, entry| {
                is_terminal_tab_id_str(tab_id) && validate_terminal_layout_snapshot(entry)
            }),
            "openFilesByWorktree" => is_record_of(value, |entry| {
                is_array_of(entry, validate_persisted_open_file)
            }),
            "browserTabsByWorktree" => is_record_of(value, |entry| {
                is_array_of(entry, validate_browser_workspace)
            }),
            "browserPagesByWorkspace" => {
                is_record_of(value, |entry| is_array_of(entry, validate_browser_page))
            }
            "unifiedTabs" => is_record_of(value, |entry| is_array_of(entry, validate_unified_tab)),
            "tabGroups" => is_record_of(value, |entry| is_array_of(entry, validate_tab_group)),
            "tabGroupLayouts" => {
                is_record_of(value, |entry| validate_tab_group_layout_node(entry, 0))
            }
            "activeWorktreeIdsOnShutdown" | "activeConnectionIdsAtShutdown" => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
            "activeFileIdByWorktree" | "activeBrowserTabIdByWorktree" | "activeTabIdByWorktree" => {
                is_record_of(value, is_nullable_string)
            }
            "markdownFrontmatterVisible" => is_record_of(value, Value::is_boolean),
            "activeTabTypeByWorktree" => is_record_of(value, |entry| {
                matches!(
                    entry.as_str(),
                    Some("terminal" | "editor" | "browser" | "simulator")
                )
            }),
            "browserUrlHistory" => is_array_of(value, validate_browser_history_entry),
            "activeGroupIdByWorktree" => is_record_of(value, Value::is_string),
            "remoteSessionIdsByTabId" => is_record_of_with_key(value, |tab_id, entry| {
                is_terminal_tab_id_str(tab_id) && entry.is_string()
            }),
            "terminalPtyIncarnationsByPaneKey" => is_record_of(value, |entry| {
                entry
                    .as_str()
                    .is_some_and(|text| !text.is_empty() && text.len() <= 128)
            }),
            "lastVisitedAtByWorktreeId" => is_record_of(value, is_nonnegative_number),
            "defaultTerminalTabsAppliedByWorktreeId" => {
                is_record_of(value, |entry| entry == &Value::Bool(true))
            }
            "sleepingAgentSessionsByPaneKey" => is_record_of_with_key(value, |pane_key, entry| {
                validate_sleeping_agent_session(pane_key, entry)
            }),
            "terminalSurfaceTombstonesByPaneKey" => {
                is_record_of(value, validate_terminal_surface_tombstone)
            }
            "terminalTopologyRevisionByRepoId" => is_record_of(value, |entry| {
                entry.as_u64().is_some() || entry.as_i64().is_some_and(|number| number >= 0)
            }),
            _ => return Err(format!("workspace session contains unknown field {name}")),
        };
        if !valid {
            return Err(format!(
                "workspace session field {name} has an invalid shape"
            ));
        }
    }
    Ok(())
}

fn is_nullable_string(value: &Value) -> bool {
    value.is_null() || value.is_string()
}

fn is_record_of(value: &Value, predicate: impl Fn(&Value) -> bool) -> bool {
    value
        .as_object()
        .is_some_and(|record| record.values().all(predicate))
}

fn is_nonnegative_number(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && number >= 0.0)
}

fn is_number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}

fn is_positive_number(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && number > 0.0)
}

fn is_array_of(value: &Value, predicate: impl Fn(&Value) -> bool) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(predicate))
}

fn is_record_of_with_key(value: &Value, predicate: impl Fn(&str, &Value) -> bool) -> bool {
    value
        .as_object()
        .is_some_and(|record| record.iter().all(|(key, entry)| predicate(key, entry)))
}

fn has_field(fields: &Map<String, Value>, name: &str, predicate: impl Fn(&Value) -> bool) -> bool {
    fields.get(name).is_some_and(predicate)
}

fn optional_field(
    fields: &Map<String, Value>,
    name: &str,
    predicate: impl Fn(&Value) -> bool,
) -> bool {
    fields.get(name).is_none_or(predicate)
}

fn is_nullable(value: &Value, predicate: impl Fn(&Value) -> bool) -> bool {
    value.is_null() || predicate(value)
}

fn is_nonempty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

fn is_terminal_tab_id(value: &Value) -> bool {
    value.as_str().is_some_and(is_terminal_tab_id_str)
}

fn is_terminal_tab_id_str(text: &str) -> bool {
    !text.is_empty() && !text.contains(':')
}

fn is_workspace_key(value: &str) -> bool {
    value
        .strip_prefix("worktree:")
        .or_else(|| value.strip_prefix("folder:"))
        .is_some_and(|suffix| !suffix.is_empty())
}

fn validate_terminal_layout_node(value: &Value, depth: usize) -> bool {
    if depth > 128 {
        return false;
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    match fields.get("type").and_then(Value::as_str) {
        Some("leaf") => has_field(fields, "leafId", Value::is_string),
        Some("split") => {
            has_field(fields, "direction", |direction| {
                matches!(direction.as_str(), Some("vertical" | "horizontal"))
            }) && has_field(fields, "first", |node| {
                validate_terminal_layout_node(node, depth + 1)
            }) && has_field(fields, "second", |node| {
                validate_terminal_layout_node(node, depth + 1)
            }) && optional_field(fields, "ratio", is_number)
        }
        _ => false,
    }
}

fn validate_terminal_layout_snapshot(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "root", |root| {
        root.is_null() || validate_terminal_layout_node(root, 0)
    }) && has_field(fields, "activeLeafId", is_nullable_string)
        && has_field(fields, "expandedLeafId", is_nullable_string)
        && [
            "ptyIdsByLeafId",
            "buffersByLeafId",
            "scrollbackRefsByLeafId",
            "titlesByLeafId",
        ]
        .iter()
        .all(|name| optional_field(fields, name, |entry| is_record_of(entry, Value::is_string)))
}

fn validate_ai_vault_title(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "agent", |agent| {
        matches!(agent.as_str(), Some("claude" | "codex"))
    }) && has_field(fields, "sessionId", Value::is_string)
        && has_field(fields, "title", Value::is_string)
}

fn is_tui_agent(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some(
            "claude"
                | "claude-agent-teams"
                | "openclaude"
                | "codex"
                | "autohand"
                | "opencode"
                | "mimo-code"
                | "pi"
                | "omp"
                | "gemini"
                | "antigravity"
                | "aider"
                | "goose"
                | "amp"
                | "kilo"
                | "kiro"
                | "crush"
                | "aug"
                | "cline"
                | "codebuff"
                | "command-code"
                | "continue"
                | "cursor"
                | "droid"
                | "kimi"
                | "mistral-vibe"
                | "qwen-code"
                | "rovo"
                | "hermes"
                | "openclaw"
                | "copilot"
                | "grok"
                | "devin"
                | "ante"
                | "trae"
                | "prime-agent"
        )
    )
}

fn validate_terminal_tab(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "id", is_terminal_tab_id)
        && has_field(fields, "ptyId", is_nullable_string)
        && has_field(fields, "worktreeId", Value::is_string)
        && has_field(fields, "title", Value::is_string)
        && has_field(fields, "customTitle", is_nullable_string)
        && has_field(fields, "color", is_nullable_string)
        && has_field(fields, "sortOrder", is_number)
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "defaultTitle", Value::is_string)
        && optional_field(fields, "generatedTitle", is_nullable_string)
        && optional_field(fields, "aiVaultTitle", |title| {
            is_nullable(title, validate_ai_vault_title)
        })
        && optional_field(fields, "quickCommandLabel", is_nullable_string)
        && optional_field(fields, "isPinned", Value::is_boolean)
        && optional_field(fields, "generation", is_number)
        && optional_field(fields, "startupCwd", is_nonempty_string)
        && optional_field(fields, "launchAgent", is_tui_agent)
}

fn validate_unified_tab(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["id", "entityId", "groupId", "worktreeId", "label"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "contentType", |content_type| {
            matches!(
                content_type.as_str(),
                Some(
                    "terminal"
                        | "editor"
                        | "diff"
                        | "conflict-review"
                        | "check-details"
                        | "browser"
                        | "simulator"
                )
            )
        })
        && has_field(fields, "customLabel", is_nullable_string)
        && has_field(fields, "color", is_nullable_string)
        && has_field(fields, "sortOrder", is_number)
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "generatedLabel", is_nullable_string)
        && optional_field(fields, "aiVaultTitle", |title| {
            is_nullable(title, validate_ai_vault_title)
        })
        && optional_field(fields, "quickCommandLabel", is_nullable_string)
        && optional_field(fields, "isPreview", Value::is_boolean)
        && optional_field(fields, "isPinned", Value::is_boolean)
        && optional_field(fields, "viewMode", |mode| {
            matches!(mode.as_str(), Some("terminal" | "chat"))
        })
}

fn validate_tab_group(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "id", Value::is_string)
        && has_field(fields, "worktreeId", Value::is_string)
        && has_field(fields, "activeTabId", is_nullable_string)
        && has_field(fields, "tabOrder", |order| {
            is_array_of(order, Value::is_string)
        })
        && optional_field(fields, "recentTabIds", |order| {
            is_array_of(order, Value::is_string)
        })
}

fn validate_tab_group_layout_node(value: &Value, depth: usize) -> bool {
    if depth > 128 {
        return false;
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    match fields.get("type").and_then(Value::as_str) {
        Some("leaf") => has_field(fields, "groupId", Value::is_string),
        Some("split") => {
            has_field(fields, "direction", |direction| {
                matches!(direction.as_str(), Some("vertical" | "horizontal"))
            }) && has_field(fields, "first", |node| {
                validate_tab_group_layout_node(node, depth + 1)
            }) && has_field(fields, "second", |node| {
                validate_tab_group_layout_node(node, depth + 1)
            }) && optional_field(fields, "ratio", is_number)
        }
        _ => false,
    }
}

fn validate_persisted_open_file(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["filePath", "relativePath", "worktreeId", "language"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && optional_field(fields, "isPreview", Value::is_boolean)
        && optional_field(fields, "runtimeEnvironmentId", is_nullable_string)
        && optional_field(fields, "externalSshTargetId", |target| {
            target.as_str().is_some_and(|text| !text.trim().is_empty())
        })
        && optional_field(fields, "dirtyDraftContent", Value::is_string)
        && optional_field(fields, "lastKnownDiskSignature", Value::is_string)
        && optional_field(fields, "readOnly", Value::is_boolean)
        && optional_field(fields, "liveTail", Value::is_boolean)
}

fn validate_browser_load_error(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "code", is_number)
        && has_field(fields, "description", Value::is_string)
        && has_field(fields, "validatedUrl", Value::is_string)
}

fn validate_browser_workspace(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["id", "worktreeId", "url", "title"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "loading", Value::is_boolean)
        && has_field(fields, "faviconUrl", is_nullable_string)
        && has_field(fields, "canGoBack", Value::is_boolean)
        && has_field(fields, "canGoForward", Value::is_boolean)
        && has_field(fields, "loadError", |error| {
            is_nullable(error, validate_browser_load_error)
        })
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "label", Value::is_string)
        && optional_field(fields, "sessionProfileId", is_nullable_string)
        && optional_field(fields, "sessionPartition", is_nullable_string)
        && optional_field(fields, "activePageId", is_nullable_string)
        && optional_field(fields, "pageIds", |ids| is_array_of(ids, Value::is_string))
}

fn validate_browser_page(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["id", "workspaceId", "worktreeId", "url", "title"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "loading", Value::is_boolean)
        && has_field(fields, "faviconUrl", is_nullable_string)
        && has_field(fields, "canGoBack", Value::is_boolean)
        && has_field(fields, "canGoForward", Value::is_boolean)
        && has_field(fields, "loadError", |error| {
            is_nullable(error, validate_browser_load_error)
        })
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "browserRuntimeEnvironmentId", is_nullable_string)
        && optional_field(fields, "viewportPresetId", |preset| {
            preset.is_null()
                || matches!(
                    preset.as_str(),
                    Some(
                        "mobile-s"
                            | "mobile-m"
                            | "mobile-l"
                            | "tablet"
                            | "laptop"
                            | "laptop-l"
                            | "desktop"
                    )
                )
        })
}

fn validate_browser_history_entry(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["url", "normalizedUrl", "title"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "lastVisitedAt", is_number)
        && has_field(fields, "visitCount", is_number)
}

fn contains_unsafe_text_chars(value: &str) -> bool {
    value
        .chars()
        .any(|character| character <= '\u{1f}' || character == '\u{7f}')
}

fn validate_provider_session(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "key", |key| {
        matches!(key.as_str(), Some("session_id" | "conversation_id"))
    }) && has_field(fields, "id", |id| {
        id.as_str().is_some_and(|text| {
            text == text.trim()
                && !text.is_empty()
                && text.len() <= 512
                && !text.starts_with('-')
                && !contains_unsafe_text_chars(text)
        })
    }) && optional_field(fields, "transcriptPath", |path| {
        path.as_str()
            .is_some_and(|text| !text.is_empty() && !contains_unsafe_text_chars(text))
    })
}

fn validate_launch_environment(value: &Value) -> bool {
    is_record_of_with_key(value, |key, entry| {
        let trimmed = key.trim();
        !trimmed.is_empty()
            && !matches!(trimmed, "__proto__" | "constructor" | "prototype")
            && !trimmed.contains('=')
            && !contains_unsafe_text_chars(trimmed)
            && entry.as_str().is_some_and(|text| !text.contains('\0'))
    })
}

fn validate_sleeping_agent_launch_config(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "agentArgs", Value::is_string)
        && has_field(fields, "agentEnv", validate_launch_environment)
        && optional_field(fields, "agentCommand", Value::is_string)
        && optional_field(fields, "ompResumeFilePath", |path| {
            path.as_str().is_some_and(|text| {
                text == text.trim()
                    && !text.is_empty()
                    && text.len() <= 32 * 1024
                    && !contains_unsafe_text_chars(text)
            })
        })
}

fn is_resumable_agent(value: &str) -> bool {
    matches!(
        value,
        "claude"
            | "codex"
            | "gemini"
            | "antigravity"
            | "opencode"
            | "pi"
            | "mimo-code"
            | "droid"
            | "grok"
            | "devin"
            | "omp"
            | "prime-agent"
    )
}

fn provider_session_matches_agent(agent: &str, provider: &Map<String, Value>) -> bool {
    let key = provider.get("key").and_then(Value::as_str);
    match agent {
        "antigravity" => key == Some("conversation_id"),
        "pi" | "prime-agent" => {
            key == Some("session_id")
                && provider
                    .get("transcriptPath")
                    .is_some_and(is_nonempty_string)
        }
        _ => key == Some("session_id"),
    }
}

fn validate_sleeping_agent_session(pane_key: &str, value: &Value) -> bool {
    if pane_key.is_empty() || matches!(pane_key, "__proto__" | "constructor" | "prototype") {
        return false;
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    let Some(agent) = fields.get("agent").and_then(Value::as_str) else {
        return false;
    };
    let Some(provider) = fields.get("providerSession").and_then(Value::as_object) else {
        return false;
    };
    has_field(fields, "paneKey", |stored| {
        stored.as_str().is_some_and(|text| text == pane_key)
    }) && has_field(fields, "worktreeId", is_nonempty_string)
        && is_resumable_agent(agent)
        && validate_provider_session(&Value::Object(provider.clone()))
        && provider_session_matches_agent(agent, provider)
        && has_field(fields, "prompt", Value::is_string)
        && has_field(fields, "state", |state| {
            matches!(
                state.as_str(),
                Some("working" | "blocked" | "waiting" | "done")
            )
        })
        && has_field(fields, "capturedAt", is_positive_number)
        && has_field(fields, "updatedAt", is_positive_number)
        && optional_field(fields, "tabId", is_terminal_tab_id)
        && optional_field(fields, "terminalTitle", Value::is_string)
        && optional_field(fields, "lastAssistantMessage", Value::is_string)
        && optional_field(fields, "interrupted", Value::is_boolean)
        && optional_field(fields, "connectionId", is_nullable_string)
        && optional_field(
            fields,
            "launchConfig",
            validate_sleeping_agent_launch_config,
        )
        && optional_field(fields, "origin", |origin| {
            matches!(origin.as_str(), Some("worktree-sleep" | "quit" | "live"))
        })
        && optional_field(fields, "automaticResumeBlockedBy", |reason| {
            reason.as_str() == Some("legacy-orchestration-worker")
        })
        && optional_field(fields, "restoreOnTabOpenOnly", Value::is_boolean)
}

fn validate_terminal_surface_tombstone(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "worktreeId", Value::is_string)
        && has_field(fields, "parentTabId", is_terminal_tab_id)
        && has_field(fields, "leafId", Value::is_string)
        && has_field(fields, "ptyId", Value::is_string)
        && has_field(fields, "incarnationId", |incarnation| {
            incarnation
                .as_str()
                .is_some_and(|text| !text.is_empty() && text.len() <= 128)
        })
        && has_field(fields, "retiredAt", is_nonnegative_number)
}
