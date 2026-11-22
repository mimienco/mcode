//! Pi-compatible family normalizer — `normalizePiCompatibleEvent`
//! (`agent-hook-listener.ts:4057-4114`) shared by pi/omp/prime-agent.

use serde_json::Value;

use crate::listener_state::{HookListenerState, clear_pane_turn_cache_state, resolve_prompt};
use crate::normalizers::ask_user::is_ask_user_question_tool;
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::is_new_turn_event;
use crate::normalizers::tool_snapshot::{
    ToolSnapshot, read_string, resolve_tool_state, tool_update,
};
use crate::types::{AgentStatusState, ParsedAgentStatusPayload};

/// The pi-family sources as wire literals (`agent-hook-relay.ts`).
pub type PiAgentKind = &'static str;

fn extract_pi_tool_fields(hook_payload: &serde_json::Map<String, Value>) -> ToolSnapshot {
    let tool_name = read_string(hook_payload, "tool_name");
    tool_update(
        tool_name.clone(),
        read_string(hook_payload, "tool_input"),
        None,
        hook_payload.contains_key("tool_input"),
    )
}

pub fn normalize_pi_compatible_event(
    state: &mut HookListenerState,
    agent_kind: PiAgentKind,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    if event_name == Some("session_start") {
        // Why: Pi-compatible session_start fires on TUI open/resume; discard
        // stale turn details, no working row before user activity.
        clear_pane_turn_cache_state(state, pane_key);
        return None;
    }
    // Why: gate on the event's own tool_name so a stale cached question can't
    // re-enter blocked.
    let tool_name = read_string(hook_payload, "tool_name");
    let is_pi_compatible_ask = ((agent_kind == "pi"
        && is_ask_user_question_tool(tool_name.as_deref()))
        || (agent_kind == "omp" && tool_name.as_deref() == Some("ask")))
        && matches!(event_name, Some("tool_call") | Some("tool_execution_start"));
    let state_name = if is_pi_compatible_ask {
        AgentStatusState::Blocked
    } else if matches!(
        event_name,
        Some(
            "before_agent_start"
                | "agent_start"
                | "tool_call"
                | "tool_execution_start"
                | "tool_execution_end"
                | "message_update"
                | "message_end"
        )
    ) {
        AgentStatusState::Working
    } else if event_name == Some("agent_end") {
        AgentStatusState::Done
    } else {
        return None;
    };
    let source = match agent_kind {
        "pi" => crate::types::AgentHookSource::Pi,
        "omp" => crate::types::AgentHookSource::Omp,
        _ => crate::types::AgentHookSource::PrimeAgent,
    };
    let snapshot = resolve_tool_state(
        &mut state.last_tool_by_pane_key,
        pane_key,
        extract_pi_tool_fields(hook_payload),
        is_new_turn_event(source, event_name),
    );
    let prompt = resolve_prompt(
        state,
        pane_key,
        prompt_text,
        is_new_turn_event(source, event_name),
    );
    normalize_agent_status_object(
        &serde_json::json!({
            "state": match state_name {
                AgentStatusState::Working => "working",
                AgentStatusState::Blocked => "blocked",
                AgentStatusState::Done => "done",
                _ => "waiting",
            },
            "prompt": prompt,
            "agentType": agent_kind,
            "toolName": snapshot.tool_name,
            "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message,
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
    fn pi_ask_gates_blocked() {
        let mut state = HookListenerState::default();
        let payload: serde_json::Map<String, Value> = json!({"tool_name": "ask_user_question"})
            .as_object()
            .unwrap()
            .clone();
        let out =
            normalize_pi_compatible_event(&mut state, "pi", Some("tool_call"), "", "p", &payload)
                .unwrap();
        assert_eq!(out.state, AgentStatusState::Blocked);
    }

    #[test]
    fn omp_asks_via_ask_tool() {
        let mut state = HookListenerState::default();
        let payload: serde_json::Map<String, Value> =
            json!({"tool_name": "ask"}).as_object().unwrap().clone();
        let out =
            normalize_pi_compatible_event(&mut state, "omp", Some("tool_call"), "", "p", &payload)
                .unwrap();
        assert_eq!(out.state, AgentStatusState::Blocked);
    }

    #[test]
    fn session_start_drops_caches() {
        let mut state = HookListenerState::default();
        state
            .last_prompt_by_pane_key
            .insert("p".into(), "old".into());
        let payload = serde_json::Map::new();
        assert!(
            normalize_pi_compatible_event(
                &mut state,
                "pi",
                Some("session_start"),
                "",
                "p",
                &payload
            )
            .is_none()
        );
        assert!(!state.last_prompt_by_pane_key.contains_key("p"));
    }

    #[test]
    fn agent_end_is_done() {
        let mut state = HookListenerState::default();
        let payload = serde_json::Map::new();
        let out = normalize_pi_compatible_event(
            &mut state,
            "pi",
            Some("agent_end"),
            "fix",
            "p",
            &payload,
        )
        .unwrap();
        assert_eq!(out.state, AgentStatusState::Done);
    }
}
