//! Codex hook lifecycle. Transcript discovery and polling are separate pending slices.

use serde_json::{Map, Value, json};

use crate::listener_state::{CodexLeadTurnState, HookListenerState, resolve_prompt};
use crate::normalizers::ask_user::is_ask_user_question_tool;
use crate::normalizers::codex_roster as roster;
use crate::normalizers::fields::{
    AGENT_MODEL_MAX_LENGTH, normalize_agent_status_object, normalize_optional_field,
};
use crate::normalizers::tool_snapshot as tool;
use crate::types::{AgentStatusState, AgentSubagentState, ParsedAgentStatusPayload};

pub fn normalize_codex_event(
    state: &mut HookListenerState,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    normalize_codex_event_at(state, event_name, prompt_text, pane_key, payload, now)
}

pub fn normalize_codex_event_at(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
    now: i64,
) -> Option<ParsedAgentStatusPayload> {
    let agent_id = tool::read_string(payload, "agent_id");
    if matches!(event, Some("SubagentStart" | "SubagentStop")) {
        let id = agent_id?;
        if event == Some("SubagentStart") {
            upsert_child(state, pane, &id, payload, AgentSubagentState::Working, now);
        } else {
            state
                .codex_subagent_rosters
                .entry(pane.to_string())
                .or_default()
                .remove(id.trim());
        }
        return build_status(state, event, "", pane, payload, false);
    }
    let question = event == Some("PreToolUse")
        && is_ask_user_question_tool(
            tool::read_first_string(payload, &["tool_name", "name"]).as_deref(),
        );
    let lead_state = match event {
        Some("PreToolUse") if question => AgentStatusState::Waiting,
        Some("SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse") => {
            AgentStatusState::Working
        }
        Some("PermissionRequest") => AgentStatusState::Waiting,
        Some("Stop") => AgentStatusState::Done,
        _ => return None,
    };
    if let Some(id) = agent_id {
        upsert_child(
            state,
            pane,
            &id,
            payload,
            if lead_state == AgentStatusState::Waiting {
                AgentSubagentState::Waiting
            } else {
                AgentSubagentState::Working
            },
            now,
        );
        return build_status(state, event, "", pane, payload, false);
    }
    // Codex 0.144 may omit child stops; hook-only lead boundaries clear the roster.
    if matches!(event, Some("SessionStart" | "Stop")) {
        state.codex_subagent_rosters.remove(pane);
    }
    let model = normalize_optional_field(
        payload.get("model").and_then(Value::as_str),
        AGENT_MODEL_MAX_LENGTH,
    )
    .or_else(|| {
        (event != Some("SessionStart"))
            .then(|| {
                state
                    .codex_lead_state_by_pane_key
                    .get(pane)
                    .and_then(|lead| lead.model.clone())
            })
            .flatten()
    });
    state.codex_lead_state_by_pane_key.insert(
        pane.to_string(),
        CodexLeadTurnState {
            state: lead_state,
            model,
        },
    );
    build_status(state, event, prompt, pane, payload, true)
}

fn upsert_child(
    state: &mut HookListenerState,
    pane: &str,
    id: &str,
    payload: &Map<String, Value>,
    child_state: AgentSubagentState,
    now: i64,
) {
    roster::upsert(
        state
            .codex_subagent_rosters
            .entry(pane.to_string())
            .or_default(),
        id,
        payload.get("agent_type").and_then(Value::as_str),
        payload.get("model").and_then(Value::as_str),
        child_state,
        now,
    );
}

fn build_status(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
    update_lead: bool,
) -> Option<ParsedAgentStatusPayload> {
    let reset = update_lead && matches!(event, Some("SessionStart" | "UserPromptSubmit"));
    let snapshot = if update_lead {
        tool::resolve_tool_state(
            &mut state.last_tool_by_pane_key,
            pane,
            extract_tool_fields(event, payload),
            reset,
        )
    } else {
        state
            .last_tool_by_pane_key
            .get(pane)
            .cloned()
            .unwrap_or_default()
    };
    let prompt = resolve_prompt(state, pane, prompt, reset);
    let lead = state.codex_lead_state_by_pane_key.get(pane);
    let effective = roster::effective_state(
        state.codex_subagent_rosters.get(pane),
        lead.map(|lead| lead.state)
            .unwrap_or(AgentStatusState::Working),
    );
    normalize_agent_status_object(
        json!({
            "state": effective,
            "prompt": prompt,
            "agentType": "codex",
            "model": lead.and_then(|lead| lead.model.as_ref()),
            "toolName": snapshot.tool_name,
            "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message,
            "subagents": roster::snapshots(state.codex_subagent_rosters.get(pane)),
        })
        .as_object()
        .unwrap(),
    )
}

fn extract_tool_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    if matches!(
        event,
        Some("PreToolUse" | "PermissionRequest" | "PostToolUse")
    ) {
        let name = tool::read_first_string(payload, &["tool_name", "name"]);
        let keys = ["tool_input", "input", "arguments"];
        let raw_input = keys
            .iter()
            .find_map(|key| payload.get(*key).filter(|value| !value.is_null()));
        let input = keys
            .iter()
            .find_map(|key| tool::derive_tool_input_preview(name.as_deref(), payload.get(*key)));
        let interactive = tool::derive_interactive_prompt(name.as_deref(), raw_input, event);
        return tool::tool_update(
            name,
            input,
            interactive,
            tool::has_any_own_field(payload, &keys),
        );
    }
    tool::ToolSnapshot {
        last_assistant_message: (event == Some("Stop"))
            .then(|| tool::read_string(payload, "last_assistant_message"))
            .flatten(),
        ..Default::default()
    }
}
