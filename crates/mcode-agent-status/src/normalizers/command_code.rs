//! Command Code direct hook fields; transcript prompt/answer recovery is pending.

use crate::listener_state::HookListenerState;
use crate::normalizers::{provider_status::normalize_cached_status, tool_snapshot as tool};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_command_code_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let state_name = match event {
        Some("PreToolUse" | "PostToolUse") => "working",
        Some("Stop") => "done",
        _ => return None,
    };
    let mut update = tool::ToolSnapshot::default();
    if matches!(event, Some("PreToolUse" | "PostToolUse")) {
        let name =
            tool::read_first_string(payload, &["tool_name", "toolName", "tool_display_name"]);
        let input = tool::derive_tool_input_preview(name.as_deref(), payload.get("tool_input"))
            .or_else(|| tool::derive_fallback_tool_input_preview(payload.get("tool_input")));
        update = tool::tool_update(name, input, None, payload.contains_key("tool_input"));
        if event == Some("PostToolUse") {
            update.last_assistant_message = ["tool_response", "tool_output"]
                .iter()
                .find_map(|key| tool::extract_tool_response_text(payload.get(*key)));
        }
    } else {
        update.last_assistant_message = tool::read_string(payload, "last_assistant_message");
    }
    normalize_cached_status(
        state,
        pane,
        prompt,
        "command-code",
        state_name,
        false,
        update,
    )
}
