//! Gemini's native and legacy hook contracts (`agent-hook-listener.ts`).

use serde_json::{Map, Value, json};

use crate::listener_state::{HookListenerState, resolve_prompt};
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::tool_snapshot as tool;
use crate::types::ParsedAgentStatusPayload;

pub fn normalize_gemini_event(
    state: &mut HookListenerState,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let state_name = match event_name {
        Some("BeforeAgent" | "BeforeTool" | "AfterTool" | "PreToolUse" | "PostToolUse") => {
            "working"
        }
        Some("AfterAgent") => "done",
        _ => return None,
    };
    let reset = event_name == Some("BeforeAgent");
    let snapshot = tool::resolve_tool_state(
        &mut state.last_tool_by_pane_key,
        pane_key,
        extract_tool_fields(event_name, payload),
        reset,
    );
    let prompt = resolve_prompt(state, pane_key, prompt_text, reset);
    normalize_agent_status_object(
        json!({
            "state": state_name,
            "prompt": prompt,
            "agentType": "gemini",
            "toolName": snapshot.tool_name,
            "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message,
        })
        .as_object()
        .unwrap(),
    )
}

fn extract_tool_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    if matches!(
        event,
        Some("BeforeTool" | "AfterTool" | "PreToolUse" | "PostToolUse")
    ) {
        let name = tool::read_first_string(payload, &["tool_name", "name"]);
        let input = ["tool_input", "args", "input"]
            .iter()
            .find_map(|key| tool::derive_tool_input_preview(name.as_deref(), payload.get(*key)));
        return tool::tool_update(
            name,
            input,
            None,
            tool::has_any_own_field(payload, &["tool_input", "args", "input"]),
        );
    }
    tool::ToolSnapshot {
        last_assistant_message: (event == Some("AfterAgent"))
            .then(|| tool::read_string(payload, "prompt_response"))
            .flatten(),
        ..Default::default()
    }
}
