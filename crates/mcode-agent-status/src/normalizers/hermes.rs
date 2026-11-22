//! Hermes native lifecycle and approval hooks.

use crate::listener_state::HookListenerState;
use crate::normalizers::{provider_status::normalize_cached_status, tool_snapshot as tool};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_hermes_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let state_name = match event {
        Some("pre_approval_request") => "waiting",
        Some("post_llm_call" | "on_session_end" | "on_session_finalize" | "on_session_reset") => {
            "done"
        }
        Some(
            "on_session_start"
            | "pre_llm_call"
            | "pre_tool_call"
            | "post_tool_call"
            | "post_approval_response",
        ) => "working",
        _ => return None,
    };
    normalize_cached_status(
        state,
        pane,
        prompt,
        "hermes",
        state_name,
        matches!(event, Some("pre_llm_call" | "on_session_start")),
        extract_fields(event, payload),
    )
}

fn extract_fields(event: Option<&str>, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    if matches!(
        event,
        Some(
            "pre_tool_call" | "post_tool_call" | "pre_approval_request" | "post_approval_response"
        )
    ) {
        let name = tool::read_first_string(payload, &["tool_name", "name"]).or_else(|| {
            matches!(
                event,
                Some("pre_approval_request" | "post_approval_response")
            )
            .then(|| "approval".to_string())
        });
        let keys = ["tool_input", "args", "input"];
        let input = keys
            .iter()
            .find_map(|key| tool::derive_tool_input_preview(name.as_deref(), payload.get(*key)))
            .or_else(|| {
                keys.iter()
                    .find_map(|key| tool::derive_fallback_tool_input_preview(payload.get(*key)))
            })
            .or_else(|| tool::read_first_string(payload, &["command", "description"]));
        let mut update = tool::tool_update(
            name,
            input,
            None,
            tool::has_any_own_field(
                payload,
                &["tool_input", "args", "input", "command", "description"],
            ),
        );
        if event == Some("post_tool_call") {
            update.last_assistant_message = ["result", "tool_response", "output"]
                .iter()
                .find_map(|key| tool::extract_tool_response_text(payload.get(*key)));
        }
        return update;
    }
    tool::ToolSnapshot {
        last_assistant_message: (event == Some("post_llm_call"))
            .then(|| {
                tool::read_first_string(
                    payload,
                    &[
                        "last_assistant_message",
                        "assistant_response",
                        "response_text",
                    ],
                )
            })
            .flatten(),
        ..Default::default()
    }
}
