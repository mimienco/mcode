//! Devin normalizer — `normalizeDevinEvent`
//! (`agent-hook-listener.ts:3215-3266`): Claude-compatible payloads, own
//! lifecycle event set.

use serde_json::Value;

use crate::listener_state::{HookListenerState, clear_pane_turn_cache_state, resolve_prompt};
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::is_new_turn_event;
use crate::normalizers::tool_snapshot as tool;
use crate::normalizers::tool_snapshot::{read_string, resolve_tool_state};
use crate::types::{AgentHookSource, AgentStatusState, ParsedAgentStatusPayload};

pub fn normalize_devin_event(
    state: &mut HookListenerState,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    if event_name == Some("SessionStart") {
        // Why: Devin emits SessionStart on idle TUI open/resume; mapping it to
        // 'working' showed a spinner before the user typed.
        clear_pane_turn_cache_state(state, pane_key);
        return None;
    }
    let state_name = match event_name {
        Some("UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostCompaction") => {
            AgentStatusState::Working
        }
        Some("PermissionRequest") => AgentStatusState::Waiting,
        Some("Stop" | "SessionEnd") => AgentStatusState::Done,
        _ => return None,
    };
    let snapshot = resolve_tool_state(
        &mut state.last_tool_by_pane_key,
        pane_key,
        extract_devin_tool_fields(event_name, hook_payload),
        is_new_turn_event(AgentHookSource::Devin, event_name),
    );
    let interrupted = event_name == Some("Stop")
        && hook_payload.get("is_interrupt").and_then(Value::as_bool) == Some(true);
    let prompt = resolve_prompt(
        state,
        pane_key,
        prompt_text,
        is_new_turn_event(AgentHookSource::Devin, event_name),
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
            "agentType": "devin",
            "toolName": snapshot.tool_name,
            "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message,
            "interrupted": interrupted,
        })
        .as_object()
        .unwrap()
        .clone(),
    )
}

/// Devin's tool fields ride the generic extractor path; the original routes
/// `extractToolFields('devin', …)` → the shared default arm.
fn extract_devin_tool_fields(
    event_name: Option<&str>,
    hook_payload: &serde_json::Map<String, Value>,
) -> tool::ToolSnapshot {
    let _ = event_name;
    let tool_name = read_string(hook_payload, "tool_name");
    tool::tool_update(
        tool_name.clone(),
        tool::read_first_string(hook_payload, &["tool_input", "args", "input"]),
        None,
        tool::has_any_own_field(hook_payload, &["tool_input", "args", "input"]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_start_is_dropped() {
        let mut state = HookListenerState::default();
        let payload = serde_json::Map::new();
        assert!(
            normalize_devin_event(&mut state, Some("SessionStart"), "", "p", &payload).is_none()
        );
    }

    #[test]
    fn stop_with_interrupt() {
        let mut state = HookListenerState::default();
        let payload: serde_json::Map<String, Value> = serde_json::json!({
            "is_interrupt": true
        })
        .as_object()
        .unwrap()
        .clone();
        let out = normalize_devin_event(&mut state, Some("Stop"), "do it", "p", &payload).unwrap();
        assert_eq!(out.state, AgentStatusState::Done);
        assert!(out.interrupted);
        assert_eq!(out.agent_type.as_deref(), Some("devin"));
    }
}
