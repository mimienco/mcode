//! Common cache-to-wire boundary for providers with a single lead turn.

use crate::listener_state::{HookListenerState, resolve_prompt};
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::tool_snapshot::{ToolSnapshot, resolve_tool_state};
use crate::types::ParsedAgentStatusPayload;

pub fn normalize_cached_status(
    state: &mut HookListenerState,
    pane: &str,
    prompt: &str,
    agent_type: &str,
    state_name: &str,
    reset: bool,
    update: ToolSnapshot,
) -> Option<ParsedAgentStatusPayload> {
    let snapshot = resolve_tool_state(&mut state.last_tool_by_pane_key, pane, update, reset);
    let prompt = resolve_prompt(state, pane, prompt, reset);
    normalize_agent_status_object(
        serde_json::json!({
            "state": state_name, "prompt": prompt, "agentType": agent_type,
            "toolName": snapshot.tool_name, "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message,
        })
        .as_object()
        .unwrap(),
    )
}
