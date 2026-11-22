//! Droid's direct hooks; transcript answer fallback remains pending.

use crate::listener_state::{HookListenerState, clear_pane_turn_cache_state};
use crate::normalizers::{provider_status::normalize_cached_status, tool_snapshot as tool};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_droid_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    if event == Some("SessionStart") {
        clear_pane_turn_cache_state(state, pane);
        return None;
    }
    let name = tool::read_first_string(payload, &["tool_name", "name"]);
    let message = tool::read_string(payload, "message")
        .unwrap_or_default()
        .to_lowercase();
    let risk = tool::read_first_string(payload, &["riskLevel", "risk_level"]).or_else(|| {
        ["tool_input", "input", "arguments"].iter().find_map(|key| {
            payload
                .get(*key)
                .and_then(Value::as_object)
                .and_then(|input| tool::read_first_string(input, &["riskLevel", "risk_level"]))
        })
    });
    let state_name = match event {
        Some("PreToolUse")
            if is_ask_user(name.as_deref())
                || risk.is_some_and(|risk| risk.trim().eq_ignore_ascii_case("high")) =>
        {
            "waiting"
        }
        Some("UserPromptSubmit" | "PreToolUse" | "PostToolUse") => "working",
        Some("Stop") => "done",
        Some("PermissionRequest") => "waiting",
        Some("Notification")
            if ["permission", "approve", "approval"]
                .iter()
                .any(|word| message.contains(word)) =>
        {
            "waiting"
        }
        Some("Notification")
            if message.contains("waiting for your input")
                || message.contains("waiting for input") =>
        {
            "done"
        }
        _ => return None,
    };
    let prompt = if event == Some("Notification") {
        ""
    } else {
        prompt
    };
    normalize_cached_status(
        state,
        pane,
        prompt,
        "droid",
        state_name,
        event == Some("UserPromptSubmit"),
        extract_fields(event, payload),
    )
}

pub fn is_ask_user(name: Option<&str>) -> bool {
    name.unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .eq_ignore_ascii_case("askuser")
}

fn extract_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    if matches!(
        event,
        Some("PreToolUse" | "PostToolUse" | "PermissionRequest")
    ) {
        let name = tool::read_first_string(payload, &["tool_name", "name"]);
        let keys = ["tool_input", "input", "arguments"];
        let input = keys
            .iter()
            .find_map(|key| tool::derive_tool_input_preview(name.as_deref(), payload.get(*key)));
        let mut update =
            tool::tool_update(name, input, None, tool::has_any_own_field(payload, &keys));
        if event == Some("PostToolUse") {
            update.last_assistant_message = ["tool_response", "tool_output"]
                .iter()
                .find_map(|key| tool::extract_tool_response_text(payload.get(*key)));
        }
        return update;
    }
    tool::ToolSnapshot {
        last_assistant_message: (event == Some("Stop"))
            .then(|| tool::read_string(payload, "last_assistant_message"))
            .flatten(),
        ..Default::default()
    }
}
