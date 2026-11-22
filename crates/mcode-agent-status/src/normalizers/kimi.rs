//! Kimi normalizer — `normalizeKimiEvent`
//! (`agent-hook-listener.ts:3274-3327`): Claude-compatible payloads; auto-
//! allowed AskUserQuestion arrives as PreToolUse (treated as waiting).

use serde_json::Value;

use crate::listener_state::{HookListenerState, resolve_prompt};
use crate::normalizers::ask_user::is_ask_user_question_tool;
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::harness_text::is_compact_continuation_user_turn_text;
use crate::normalizers::is_new_turn_event;
use crate::normalizers::tool_snapshot::{
    ToolSnapshot, read_string, resolve_tool_state, tool_update,
};
use crate::types::{AgentHookSource, AgentStatusState, ParsedAgentStatusPayload};

fn extract_kimi_tool_fields(
    event_name: Option<&str>,
    hook_payload: &serde_json::Map<String, Value>,
) -> ToolSnapshot {
    let _ = event_name;
    let tool_name = read_string(hook_payload, "tool_name");
    tool_update(
        tool_name.clone(),
        read_string(hook_payload, "tool_input"),
        None,
        hook_payload.contains_key("tool_input"),
    )
}

pub fn normalize_kimi_event(
    state: &mut HookListenerState,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    if event_name == Some("UserPromptSubmit") && is_compact_continuation_user_turn_text(prompt_text)
    {
        return None;
    }
    let tool_name = read_string(hook_payload, "tool_name");
    let is_user_input_tool = is_ask_user_question_tool(tool_name.as_deref());
    let state_name = if matches!(
        event_name,
        Some("UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure")
    ) || (event_name == Some("PreToolUse") && !is_user_input_tool)
    {
        AgentStatusState::Working
    } else if event_name == Some("PermissionRequest")
        || (event_name == Some("PreToolUse") && is_user_input_tool)
    {
        AgentStatusState::Waiting
    } else if matches!(event_name, Some("Stop") | Some("StopFailure")) {
        AgentStatusState::Done
    } else {
        return None;
    };
    let snapshot = resolve_tool_state(
        &mut state.last_tool_by_pane_key,
        pane_key,
        extract_kimi_tool_fields(event_name, hook_payload),
        is_new_turn_event(AgentHookSource::Kimi, event_name),
    );
    let interrupted = event_name == Some("Stop")
        && hook_payload.get("is_interrupt").and_then(Value::as_bool) == Some(true);
    let prompt = resolve_prompt(
        state,
        pane_key,
        prompt_text,
        is_new_turn_event(AgentHookSource::Kimi, event_name),
    );
    normalize_agent_status_object(
        &serde_json::json!({
            "state": match state_name {
                AgentStatusState::Working => "working",
                AgentStatusState::Waiting => "waiting",
                AgentStatusState::Done => "done",
                _ => "blocked",
            },
            "prompt": prompt,
            "agentType": "kimi",
            "toolName": snapshot.tool_name,
            "toolInput": snapshot.tool_input,
            "lastAssistantMessage": snapshot.last_assistant_message,
            "interrupted": interrupted,
        })
        .as_object()
        .unwrap()
        .clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pre_tool_use_ask_user_question_is_waiting() {
        let mut state = HookListenerState::default();
        let payload: serde_json::Map<String, Value> = json!({
            "tool_name": "AskUserQuestion",
            "tool_input": "{\"questions\":[{\"question\":\"Q?\"}]}"
        })
        .as_object()
        .unwrap()
        .clone();
        let out = normalize_kimi_event(&mut state, Some("PreToolUse"), "", "p", &payload).unwrap();
        assert_eq!(out.state, AgentStatusState::Waiting);
    }

    #[test]
    fn plain_pre_tool_use_is_working() {
        let mut state = HookListenerState::default();
        let payload: serde_json::Map<String, Value> =
            json!({"tool_name": "Bash"}).as_object().unwrap().clone();
        let out =
            normalize_kimi_event(&mut state, Some("PreToolUse"), "ship it", "p", &payload).unwrap();
        assert_eq!(out.state, AgentStatusState::Working);
        assert_eq!(out.prompt, "ship it");
    }

    #[test]
    fn compact_continuation_prompt_ignored() {
        let mut state = HookListenerState::default();
        let payload = serde_json::Map::new();
        let text = "This session is being continued from a previous conversation";
        assert!(
            normalize_kimi_event(&mut state, Some("UserPromptSubmit"), text, "p", &payload)
                .is_none()
        );
    }
}
