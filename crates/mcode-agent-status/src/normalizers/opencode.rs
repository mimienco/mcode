//! OpenCode and MiMo permission/question hooks retire answered tool fields.

use crate::listener_state::HookListenerState;
use crate::normalizers::{provider_status::normalize_cached_status, tool_snapshot as tool};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_opencode_event(
    state: &mut HookListenerState,
    source: &str,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let state_name = match event {
        Some("SessionBusy" | "MessagePart") => "working",
        Some("SessionIdle") => "done",
        Some("PermissionRequest" | "AskUserQuestion") => "waiting",
        _ => return None,
    };
    normalize_cached_status(
        state,
        pane,
        prompt,
        source,
        state_name,
        false,
        extract_fields(event, payload),
    )
}

fn extract_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    let mut update = tool::clear_active_tool_fields_update();
    if event == Some("MessagePart")
        && payload.get("role").and_then(Value::as_str) == Some("assistant")
    {
        update.last_assistant_message = tool::read_string(payload, "text")
            .map(|text| super::fields::truncate_utf16(&text, 8_000));
    } else if event == Some("AskUserQuestion") {
        let mut stripped = payload.clone();
        stripped.remove("hook_event_name");
        stripped.remove("hookEventName");
        let stripped = Value::Object(stripped);
        let input = payload.get("tool_input").unwrap_or(&stripped);
        update.interactive_prompt =
            tool::derive_interactive_prompt(Some("AskUserQuestion"), Some(input), None);
    } else if event == Some("PermissionRequest") {
        let metadata = payload
            .get("metadata")
            .and_then(Value::as_object)
            .and_then(|metadata| {
                tool::read_first_string(
                    metadata,
                    &["command", "filepath", "file_path", "path", "url"],
                )
            });
        let patterns = payload
            .get("patterns")
            .and_then(Value::as_array)
            .map(|patterns| {
                patterns
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|value| !value.is_empty());
        update = tool::tool_update(
            tool::read_string(payload, "permission"),
            metadata.or(patterns),
            None,
            tool::has_any_own_field(payload, &["metadata", "patterns"]),
        );
    }
    update
}
