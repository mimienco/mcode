//! Cursor native hooks, including late final answers and execution gates.

use serde_json::{Map, Value, json};

use crate::listener_state::{HookListenerState, resolve_prompt};
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::tool_snapshot as tool;
use crate::types::{AgentStatusState, ParsedAgentStatusPayload};

pub fn normalize_cursor_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let state_name = match event {
        Some(
            "beforeSubmitPrompt"
            | "sessionStart"
            | "preToolUse"
            | "postToolUse"
            | "postToolUseFailure"
            | "beforeShellExecution"
            | "beforeMCPExecution",
        ) => "working",
        Some("afterAgentResponse") => {
            if state
                .last_status_by_pane_key
                .get(pane)
                .is_some_and(|previous| {
                    previous.payload.state == AgentStatusState::Done
                        && previous.payload.agent_type.as_deref() == Some("cursor")
                })
            {
                "done"
            } else {
                "working"
            }
        }
        Some("stop" | "sessionEnd") => "done",
        _ => return None,
    };
    let reset = matches!(event, Some("beforeSubmitPrompt" | "sessionStart"));
    let snapshot = tool::resolve_tool_state(
        &mut state.last_tool_by_pane_key,
        pane,
        extract_tool_fields(event, payload),
        reset,
    );
    let prompt = resolve_prompt(state, pane, prompt, reset);
    let interrupted = event == Some("stop")
        && payload
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status != "completed");
    normalize_agent_status_object(
        json!({
            "state": state_name, "prompt": prompt, "agentType": "cursor",
            "toolName": snapshot.tool_name, "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message, "interrupted": interrupted,
        })
        .as_object()
        .unwrap(),
    )
}

fn extract_tool_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    match event {
        Some("preToolUse" | "postToolUse" | "postToolUseFailure") => {
            let mut update = if event == Some("postToolUseFailure") {
                tool::clear_active_tool_fields_update()
            } else {
                let name = tool::read_string(payload, "tool_name");
                let input =
                    tool::derive_tool_input_preview(name.as_deref(), payload.get("tool_input"));
                tool::tool_update(name, input, None, payload.contains_key("tool_input"))
            };
            update.last_assistant_message = match event {
                Some("postToolUse") => tool::extract_tool_response_text(payload.get("tool_output")),
                Some("postToolUseFailure") => {
                    tool::extract_tool_response_text(payload.get("tool_output"))
                        .or_else(|| tool::read_first_string(payload, &["error_message", "error"]))
                }
                _ => None,
            };
            update
        }
        Some("beforeShellExecution") => tool::tool_update(
            Some("Shell".to_string()),
            tool::read_string(payload, "command"),
            None,
            payload.contains_key("command"),
        ),
        Some("beforeMCPExecution") => {
            let name = tool::read_string(payload, "tool_name").unwrap_or_else(|| "MCP".to_string());
            let input = tool::derive_tool_input_preview(Some(&name), payload.get("tool_input"))
                .or_else(|| tool::read_first_string(payload, &["command", "url"]));
            tool::tool_update(
                Some(name),
                input,
                None,
                tool::has_any_own_field(payload, &["tool_input", "command", "url"]),
            )
        }
        Some("afterAgentResponse") => tool::ToolSnapshot {
            last_assistant_message: tool::read_string(payload, "text"),
            ..Default::default()
        },
        _ => tool::ToolSnapshot::default(),
    }
}
