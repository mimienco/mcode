//! Direct hook normalizers for all 18 provider sources. Filesystem transcript
//! recovery, Codex polling and remote reconciliation remain pending.

pub mod amp;
pub mod antigravity;
pub mod ask_user;
pub mod claude;
pub mod claude_roster;
pub mod codex;
pub mod codex_roster;
pub mod command_code;
pub mod copilot;
pub mod cursor;
pub mod droid;
pub mod fields;
pub mod gemini;
pub mod grok;
pub mod harness_text;
pub mod hermes;
pub mod opencode;
mod provider_status;
pub mod tool_snapshot;

use crate::listener_state::HookListenerState;
use crate::types::{AgentHookEventPayload, AgentHookSource, ParsedAgentStatusPayload};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizeError {
    /// No status-shaped payload for this event (the original returns null —
    /// not an error; the HTTP layer still answers 204 fail-open).
    NoPayload,
    /// Wave-38 provider tranche.
    UnsupportedProvider(AgentHookSource),
}

/// Per-provider session extraction — `agent-session-resume.ts:182-238` for the
/// wave-37 sources (claude/codex-shaped session_id + transcript_path).
pub fn extract_provider_session(
    source: AgentHookSource,
    payload: &serde_json::Map<String, serde_json::Value>,
) -> Option<crate::types::AgentProviderSessionMetadata> {
    use crate::types::AgentProviderSessionMetadata;
    let key_literals: &[&str] = match source {
        AgentHookSource::Claude | AgentHookSource::Codex => &["session_id"],
        AgentHookSource::Gemini | AgentHookSource::Droid | AgentHookSource::Kimi => &["session_id"],
        AgentHookSource::Devin => &["session_id", "sessionId"],
        AgentHookSource::Grok => &["sessionId", "session_id"],
        AgentHookSource::Antigravity => &["conversationId"],
        AgentHookSource::Opencode | AgentHookSource::MimoCode => &["sessionID"],
        AgentHookSource::Pi | AgentHookSource::PrimeAgent | AgentHookSource::Omp => &["session_id"],
        _ => return None,
    };
    let id = key_literals.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| {
                !value.is_empty()
                    && value.encode_utf16().count() <= 512
                    && !value.starts_with('-')
                    && !value.bytes().any(|b| b <= 0x1f || b == 0x7f)
            })
    })?;
    let mut metadata = AgentProviderSessionMetadata {
        key: if source == AgentHookSource::Antigravity {
            "conversation_id"
        } else {
            "session_id"
        }
        .to_string(),
        id: id.to_string(),
        transcript_path: None,
    };
    if matches!(
        source,
        AgentHookSource::Claude
            | AgentHookSource::Codex
            | AgentHookSource::Pi
            | AgentHookSource::PrimeAgent
            | AgentHookSource::Omp
    ) {
        let path_keys: &[&str] = if matches!(
            source,
            AgentHookSource::Pi | AgentHookSource::PrimeAgent | AgentHookSource::Omp
        ) {
            &["session_file"]
        } else {
            &["transcript_path", "transcriptPath"]
        };
        metadata.transcript_path = path_keys.iter().find_map(|key| {
            payload
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty() && !value.bytes().any(|byte| byte <= 0x1f || byte == 0x7f)
                })
                .map(str::to_string)
        });
    }
    // Why: Pi/prime-agent transcript session_start carries resume identity;
    // without a session_file it is not authoritative.
    if matches!(source, AgentHookSource::Pi | AgentHookSource::PrimeAgent)
        && metadata.transcript_path.is_none()
    {
        return None;
    }
    Some(metadata)
}

/// `normalizeClaudePromptId` (`agent-hook-listener.ts:118-124`).
pub fn normalize_claude_prompt_id(value: Option<&str>) -> Option<String> {
    let value = value?.trim().to_lowercase();
    let bytes = value.as_bytes();
    let is_hex_uuid = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    is_hex_uuid.then_some(value)
}

/// Envelope + per-source dispatch — `normalizeHookPayload`
/// (`agent-hook-listener.ts:4358-4658`). Returns the transport payload.
pub fn normalize_hook_payload(
    state: &mut HookListenerState,
    source: AgentHookSource,
    body: &serde_json::Map<String, serde_json::Value>,
    raw_payload_string: Option<&str>,
    expected_env: &str,
) -> Result<AgentHookEventPayload, NormalizeError> {
    let pane_key = body
        .get("paneKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let parsed = crate::types::parse_pane_key(pane_key);
    // Why (#15117): Antigravity events can arrive with no payload at all.
    let payload_absent = raw_payload_string.is_none_or(|raw| raw.trim().is_empty())
        && matches!(source, AgentHookSource::Antigravity);
    let hook_payload: serde_json::Map<String, serde_json::Value> = if payload_absent {
        serde_json::Map::new()
    } else if let Some(raw) = raw_payload_string {
        let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        serde_json::from_str(raw).map_err(|_| NormalizeError::NoPayload)?
    } else {
        serde_json::Map::new()
    };
    if pane_key.is_empty() || pane_key.len() > crate::types::MAX_PANE_KEY_LEN || parsed.is_none() {
        return Err(NormalizeError::NoPayload);
    }

    warn_on_hook_env_or_version_mismatch(
        state,
        body.get("version").and_then(serde_json::Value::as_str),
        body.get("env").and_then(serde_json::Value::as_str),
        expected_env,
    );

    let tab_id = body
        .get("tabId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let (parsed_tab_id, _) = parsed.unwrap();
    if let Some(tab_id) = &tab_id
        && tab_id != &parsed_tab_id
    {
        return Err(NormalizeError::NoPayload);
    }
    let worktree_id = body
        .get("worktreeId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let launch_token = body
        .get("launchToken")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty());

    if source == AgentHookSource::Claude {
        state
            .claude_unconfirmed_restored_status_pane_keys
            .remove(pane_key);
    }
    let event_name = ["hook_event_name", "hookEventName", "hook_type", "hookType"]
        .iter()
        .find_map(|key| body.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .or_else(|| {
            hook_payload
                .get("hook_event_name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            hook_payload
                .get("hookEventName")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    // Why: Codex child hooks expose the child's session_id on the parent's pane.
    let provider_session = if source == AgentHookSource::Codex
        && tool_snapshot::read_string(&hook_payload, "agent_id").is_some()
    {
        None
    } else {
        extract_provider_session(source, &hook_payload)
    };
    let provider_prompt_id = if source == AgentHookSource::Claude {
        normalize_claude_prompt_id(
            hook_payload
                .get("prompt_id")
                .and_then(serde_json::Value::as_str),
        )
    } else {
        None
    };
    let compact_trigger = if source == AgentHookSource::Claude
        && matches!(
            event_name.as_deref(),
            Some("PreCompact") | Some("PostCompact")
        )
        && matches!(
            hook_payload
                .get("trigger")
                .and_then(serde_json::Value::as_str),
            Some("manual") | Some("auto")
        ) {
        hook_payload
            .get("trigger")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    } else {
        None
    };
    let is_compact_event = matches!(
        event_name.as_deref(),
        Some("PreCompact") | Some("PostCompact")
    );
    if is_compact_event && compact_trigger.is_none() {
        return Err(NormalizeError::NoPayload);
    }
    let previous_status = state.last_status_by_pane_key.get(pane_key).cloned();
    if compact_trigger.is_some()
        && !can_accept_claude_compact_transition(
            &previous_status,
            event_name.as_deref(),
            provider_prompt_id.as_deref(),
            compact_trigger.as_deref(),
            provider_session.as_ref(),
        )
    {
        return Err(NormalizeError::NoPayload);
    }
    if event_name.as_deref() == Some("PostCompact")
        && compact_trigger.is_some()
        && previous_status
            .as_ref()
            .is_some_and(|status| !status.payload.prompt.is_empty())
        && !state.last_prompt_by_pane_key.contains_key(pane_key)
    {
        let prompt = previous_status.as_ref().unwrap().payload.prompt.clone();
        state
            .last_prompt_by_pane_key
            .insert(pane_key.to_string(), prompt);
    }

    let (prompt_text, prompt_source) = extract_prompt_text(&hook_payload);
    let payload = match source {
        AgentHookSource::Amp => amp::normalize_amp_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Antigravity => antigravity::normalize_antigravity_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::CommandCode => command_code::normalize_command_code_event(
            state,
            event_name.as_deref(),
            if prompt_source == "message" {
                ""
            } else {
                &prompt_text
            },
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Copilot => copilot::normalize_copilot_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Droid => droid::normalize_droid_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Grok => grok::normalize_grok_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Hermes => hermes::normalize_hermes_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Opencode | AgentHookSource::MimoCode => {
            opencode::normalize_opencode_event(
                state,
                source.as_str(),
                event_name.as_deref(),
                &prompt_text,
                pane_key,
                &hook_payload,
            )
        }
        AgentHookSource::Cursor => cursor::normalize_cursor_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Codex => codex::normalize_codex_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Gemini => gemini::normalize_gemini_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Claude => claude::normalize_claude_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Devin => devin::normalize_devin_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Kimi => kimi::normalize_kimi_event(
            state,
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Pi => pi_family::normalize_pi_compatible_event(
            state,
            "pi",
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::Omp => pi_family::normalize_pi_compatible_event(
            state,
            "omp",
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
        AgentHookSource::PrimeAgent => pi_family::normalize_pi_compatible_event(
            state,
            "prime-agent",
            event_name.as_deref(),
            &prompt_text,
            pane_key,
            &hook_payload,
        ),
    };

    let provider_session_only = matches!(
        source,
        AgentHookSource::Pi | AgentHookSource::Omp | AgentHookSource::PrimeAgent
    ) && event_name.as_deref() == Some("session_start")
        && provider_session.is_some();
    let transport_payload = match payload {
        Some(payload) => Some(payload),
        None if provider_session_only => Some(ParsedAgentStatusPayload {
            state: crate::types::AgentStatusState::Done,
            agent_type: Some(source.as_str().to_string()),
            ..ParsedAgentStatusPayload::default()
        }),
        None => None,
    };
    let Some(transport_payload) = transport_payload else {
        return Err(NormalizeError::NoPayload);
    };
    let restored_unconfirmed = source == AgentHookSource::Claude
        && state
            .claude_unconfirmed_restored_status_pane_keys
            .remove(pane_key);
    Ok(AgentHookEventPayload {
        pane_key: pane_key.to_string(),
        source: Some(source),
        launch_token,
        tab_id,
        worktree_id,
        connection_id: None,
        restored_unconfirmed,
        has_explicit_prompt: has_explicit_user_prompt(
            source,
            event_name.as_deref(),
            &prompt_text,
            prompt_source,
        ),
        prompt_interaction_key: if matches!(
            source,
            AgentHookSource::Opencode | AgentHookSource::MimoCode
        ) && prompt_source == "role_user_text"
        {
            tool_snapshot::read_first_string(
                &hook_payload,
                &["messageID", "messageId", "message_id"],
            )
            .map(|id| format!("{}-message-{id}", source.as_str()))
        } else {
            None
        },
        hook_event_name: event_name.clone(),
        provider_prompt_id,
        compact_trigger,
        tool_use_id: ["tool_use_id", "toolUseId"].iter().find_map(|key| {
            hook_payload
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        }),
        tool_agent_id: ["agent_id", "agentId"].iter().find_map(|key| {
            hook_payload
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        }),
        teammate_name: if source == AgentHookSource::Claude
            && event_name.as_deref() == Some("TeammateIdle")
        {
            hook_payload
                .get("teammate_name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        } else {
            None
        },
        tool_agent_type: hook_payload
            .get("agent_type")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        claude_running_non_agent_task: source == AgentHookSource::Claude
            && (state
                .claude_running_non_agent_task_pane_keys
                .contains(pane_key)
                || state
                    .claude_active_session_cron_pane_keys
                    .contains(pane_key)),
        provider_session,
        provider_session_only,
        is_replay: false,
        payload: transport_payload,
    })
}

/// `canAcceptClaudeCompactTransition` (`agent-hook-listener.ts:381-417`),
/// local-host shape (connectionId always null under Tauri).
fn can_accept_claude_compact_transition(
    previous: &Option<AgentHookEventPayload>,
    event_name: Option<&str>,
    provider_prompt_id: Option<&str>,
    compact_trigger: Option<&str>,
    provider_session: Option<&crate::types::AgentProviderSessionMetadata>,
) -> bool {
    let Some(incoming_trigger) = compact_trigger else {
        return false;
    };
    let Some(incoming_prompt_id) = provider_prompt_id else {
        return false;
    };
    let Some(event) = event_name else {
        return false;
    };
    if event != "PreCompact" && event != "PostCompact" {
        return false;
    }
    let Some(previous) = previous else {
        return false;
    };
    if previous.source != Some(AgentHookSource::Claude)
        || previous.payload.agent_type.as_deref() != Some("claude")
        || previous.connection_id.is_some()
        || !provider_sessions_equal(previous.provider_session.as_ref(), provider_session)
    {
        return false;
    }
    if event == "PostCompact" {
        return previous.compact_trigger.as_deref() == Some(incoming_trigger)
            && previous.provider_prompt_id.as_deref() == Some(incoming_prompt_id);
    }
    if incoming_trigger == "manual" {
        previous.provider_prompt_id.is_some()
    } else {
        previous.provider_prompt_id.as_deref() == Some(incoming_prompt_id)
    }
}

fn provider_sessions_equal(
    left: Option<&crate::types::AgentProviderSessionMetadata>,
    right: Option<&crate::types::AgentProviderSessionMetadata>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(l), Some(r)) => l.key == r.key && l.id == r.id,
        _ => false,
    }
}

/// `warnOnHookEnvOrVersionMismatch` (`agent-hook-listener.ts:300-327`) —
/// warn-once per (version|env) per listener state; warn happens at the log
/// layer (eprintln) exactly like the original's console.warn.
fn warn_on_hook_env_or_version_mismatch(
    state: &mut HookListenerState,
    version: Option<&str>,
    env: Option<&str>,
    expected_env: &str,
) {
    if let Some(version) = version.filter(|v| *v != crate::types::MCODE_HOOK_PROTOCOL_VERSION)
        && state.warned_versions.insert(version.to_string())
    {
        eprintln!(
            "[agent-hooks] hook payload version mismatch: got {version}, expected {}",
            crate::types::MCODE_HOOK_PROTOCOL_VERSION
        );
    }
    if let Some(env) = env.filter(|e| *e != expected_env)
        && state.warned_envs.insert(env.to_string())
    {
        eprintln!("[agent-hooks] hook env mismatch: got {env}, expected {expected_env}");
    }
}

/// `extractPromptText` (`agent-hook-listener.ts:596-628`).
fn extract_prompt_text(
    hook_payload: &serde_json::Map<String, serde_json::Value>,
) -> (String, &'static str) {
    for key in [
        "prompt",
        "user_prompt",
        "userPrompt",
        "initial_prompt",
        "initialPrompt",
        "user_message",
        "message",
    ] {
        if let Some(value) = hook_payload.get(key) {
            if let Some(text) = value.as_str() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return (trimmed.to_string(), key);
                }
            }
            // Why: Kimi sends `prompt` as a content-block array, not a string.
            if key != "message"
                && let Some(text) = content_block_array_text(value)
                && !text.is_empty()
            {
                return (text, key);
            }
        }
    }
    // Why: OpenCode sends MessagePart { role, text }; not wave-37 sources but
    // keep the shape for parity.
    if hook_payload.get("role").and_then(serde_json::Value::as_str) == Some("user")
        && let Some(text) = hook_payload.get("text").and_then(serde_json::Value::as_str)
    {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return (trimmed.to_string(), "role_user_text");
        }
    }
    (String::new(), "")
}

fn content_block_array_text(value: &serde_json::Value) -> Option<String> {
    let items = value.as_array()?;
    let mut parts: Vec<String> = Vec::new();
    for item in items {
        if let Some(text) = item.as_str() {
            parts.push(text.to_string());
        } else if let Some(text) = item.get("text").and_then(serde_json::Value::as_str) {
            parts.push(text.to_string());
        }
    }
    let joined = parts.join(" ");
    let collapsed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

/// `hasExplicitUserPrompt` (`agent-hook-listener.ts:2527-2572`) — the
/// wave-37-relevant subset (no transcript-evidence providers here).
fn has_explicit_user_prompt(
    source: AgentHookSource,
    event_name: Option<&str>,
    prompt_text: &str,
    prompt_source: &str,
) -> bool {
    if source == AgentHookSource::CommandCode
        && matches!(event_name, Some("PreToolUse" | "Stop"))
        && prompt_source != "message"
        && !prompt_text.trim().is_empty()
    {
        return true;
    }
    if source == AgentHookSource::Antigravity
        && is_new_turn_event(source, event_name)
        && !prompt_text.trim().is_empty()
    {
        return true;
    }
    if prompt_source == "role_user_text" {
        return matches!(
            source,
            AgentHookSource::Opencode | AgentHookSource::MimoCode
        ) && event_name == Some("MessagePart");
    }
    if prompt_source == "message" {
        return false;
    }
    if prompt_text.is_empty() {
        return false;
    }
    if crate::normalizers::harness_text::is_known_harness_injected_user_turn_text(prompt_text) {
        return false;
    }
    is_new_turn_event(source, event_name)
}

/// `isNewTurnEvent` (`agent-hook-listener.ts:2482-2525`) — wave-37 sources.
pub fn is_new_turn_event(source: AgentHookSource, event_name: Option<&str>) -> bool {
    match source {
        AgentHookSource::Claude => {
            matches!(event_name, Some("SessionStart") | Some("UserPromptSubmit"))
        }
        AgentHookSource::Kimi | AgentHookSource::Devin | AgentHookSource::Droid => {
            event_name == Some("UserPromptSubmit")
        }
        AgentHookSource::Codex => {
            matches!(event_name, Some("SessionStart") | Some("UserPromptSubmit"))
        }
        AgentHookSource::Gemini => event_name == Some("BeforeAgent"),
        AgentHookSource::Cursor => {
            matches!(event_name, Some("beforeSubmitPrompt" | "sessionStart"))
        }
        AgentHookSource::Amp => event_name == Some("agent.start"),
        AgentHookSource::Antigravity => event_name == Some("PreInvocation"),
        AgentHookSource::Hermes => matches!(event_name, Some("pre_llm_call" | "on_session_start")),
        AgentHookSource::Copilot => matches!(
            copilot::normalize_event_name(event_name.unwrap_or_default()),
            "SessionStart" | "UserPromptSubmit"
        ),
        AgentHookSource::Grok => {
            harness_text::normalize_hook_event_name(event_name.unwrap_or_default())
                == "user_prompt_submit"
        }
        AgentHookSource::Pi | AgentHookSource::Omp | AgentHookSource::PrimeAgent => {
            event_name == Some("before_agent_start")
        }
        AgentHookSource::CommandCode => false,
        _ => false,
    }
}

pub mod devin;
pub mod kimi;
pub mod pi_family;
