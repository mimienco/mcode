//! Grok direct hooks; transcript/chat-history discovery remains pending.

use crate::listener_state::{HookListenerState, clear_pane_turn_cache_state};
use crate::normalizers::{
    ask_user::is_ask_user_question_tool, harness_text::normalize_hook_event_name,
    provider_status::normalize_cached_status, tool_snapshot as tool,
};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_grok_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let normalized = normalize_hook_event_name(event.unwrap_or_default());
    let event = normalized.as_str();
    if event == "session_start" {
        clear_pane_turn_cache_state(state, pane);
        return None;
    }
    let name = tool::read_first_string(payload, &["toolName", "tool_name", "name"]);
    let message = tool::read_string(payload, "message")
        .unwrap_or_default()
        .to_lowercase();
    let notification_type =
        tool::read_first_string(payload, &["notificationType", "notification_type", "type"]);
    let level = tool::read_string(payload, "level");
    let state_name = match event {
        "pre_tool_use" if is_ask_user_question_tool(name.as_deref()) => "waiting",
        "user_prompt_submit" | "pre_tool_use" | "post_tool_use" | "post_tool_use_failure" => {
            "working"
        }
        "stop" | "session_end" | "stop_failure" => "done",
        "notification"
            if normalize_hook_event_name(notification_type.as_deref().unwrap_or_default())
                == "permission_prompt"
                && message.trim() == "tool permission requested"
                && level.is_none_or(|level| level.trim().eq_ignore_ascii_case("info")) =>
        {
            return None;
        }
        "notification"
            if [
                "permission",
                "approval",
                "approve",
                "allow",
                "confirm",
                "needs your",
                "requires your",
                "feedback",
                "clarify",
                "question",
            ]
            .iter()
            .any(|word| message.contains(word)) =>
        {
            "waiting"
        }
        "notification"
            if [
                "type your message",
                "enter send",
                "shift-tab normal",
                "ask a side question",
            ]
            .iter()
            .any(|word| message.contains(word)) =>
        {
            "done"
        }
        _ => return None,
    };
    let prompt = if event == "notification" {
        ""
    } else if let Some(wrapped) = prompt.strip_prefix("<user_query>") {
        wrapped
            .strip_suffix("</user_query>")
            .unwrap_or(wrapped)
            .trim()
    } else {
        prompt
    };
    normalize_cached_status(
        state,
        pane,
        prompt,
        "grok",
        state_name,
        event == "user_prompt_submit",
        extract_fields(event, payload),
    )
}

fn extract_fields(event: &str, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    if matches!(
        event,
        "pre_tool_use" | "post_tool_use" | "post_tool_use_failure"
    ) {
        let mut update = if event == "post_tool_use_failure" {
            tool::clear_active_tool_fields_update()
        } else {
            let name = tool::read_first_string(payload, &["toolName", "tool_name", "name"]);
            let keys = ["toolInput", "tool_input", "input", "arguments"];
            let raw = keys
                .iter()
                .find_map(|key| payload.get(*key).filter(|value| !value.is_null()));
            let input = tool::derive_tool_input_preview(name.as_deref(), raw)
                .or_else(|| tool::derive_fallback_tool_input_preview(raw));
            let interactive = tool::derive_interactive_prompt(name.as_deref(), raw, Some(event));
            tool::tool_update(
                name,
                input,
                interactive,
                tool::has_any_own_field(payload, &keys),
            )
        };
        if event != "pre_tool_use" {
            update.last_assistant_message =
                ["toolResponse", "tool_response", "toolOutput", "tool_output"]
                    .iter()
                    .find_map(|key| tool::extract_tool_response_text(payload.get(*key)))
                    .or_else(|| tool::read_first_string(payload, &["error", "message"]));
        }
        return update;
    }
    tool::ToolSnapshot {
        last_assistant_message: matches!(event, "stop" | "session_end" | "stop_failure")
            .then(|| {
                tool::read_first_string(
                    payload,
                    &["lastAssistantMessage", "last_assistant_message"],
                )
            })
            .flatten(),
        ..Default::default()
    }
}
