//! Antigravity direct hooks and completion fences; transcript fallback is pending.

use crate::listener_state::HookListenerState;
use crate::normalizers::{provider_status::normalize_cached_status, tool_snapshot as tool};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_antigravity_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let transcript = tool::read_first_string(payload, &["transcriptPath", "transcript_path"]);
    if event == Some("PreInvocation") {
        state
            .antigravity_completed_transcript_by_pane_key
            .remove(pane);
    } else if event != Some("Stop")
        && transcript.as_ref().is_some_and(|path| {
            state.antigravity_completed_transcript_by_pane_key.get(pane) == Some(path)
        })
    {
        return None;
    }
    let tool_call = payload.get("toolCall").and_then(Value::as_object);
    let name = tool_call
        .and_then(|call| tool::read_first_string(call, &["name", "toolName", "tool_name"]));
    let busy = event == Some("Stop")
        && ["fullyIdle", "fully_idle"]
            .iter()
            .any(|key| payload.get(*key).and_then(Value::as_bool) == Some(false));
    let state_name = match event {
        Some("PreToolUse")
            if matches!(name.as_deref(), Some("ask_question" | "ask_permission")) =>
        {
            "waiting"
        }
        Some("Stop") if !busy => "done",
        Some("Stop" | "PreInvocation" | "PostInvocation" | "PreToolUse" | "PostToolUse") => {
            "working"
        }
        _ => return None,
    };
    let update = if matches!(event, Some("PreToolUse" | "PostToolUse")) {
        let raw = tool_call.and_then(|call| call.get("args"));
        let input = tool::derive_tool_input_preview(name.as_deref(), raw)
            .or_else(|| tool::derive_fallback_tool_input_preview(raw));
        tool::tool_update(name, input, None, raw.is_some())
    } else {
        tool::ToolSnapshot {
            last_assistant_message: (event == Some("Stop") && !busy)
                .then(|| tool::read_string(payload, "last_assistant_message"))
                .flatten(),
            ..Default::default()
        }
    };
    let result = normalize_cached_status(
        state,
        pane,
        prompt,
        "antigravity",
        state_name,
        event == Some("PreInvocation"),
        update,
    );
    if event == Some("Stop")
        && !busy
        && let Some(path) = transcript
    {
        state
            .antigravity_completed_transcript_by_pane_key
            .insert(pane.to_string(), path);
    }
    result
}
