//! Copilot hook aliases, inferred event names and direct response fields.
//! Stop transcript fallback is pending; a missing direct answer clears stale output.

use crate::listener_state::HookListenerState;
use crate::normalizers::{
    droid::is_ask_user, provider_status::normalize_cached_status, tool_snapshot as tool,
};
use crate::types::ParsedAgentStatusPayload;
use serde_json::{Map, Value};

pub fn normalize_event_name(event: &str) -> &str {
    match event {
        "sessionStart" => "SessionStart",
        "sessionEnd" => "SessionEnd",
        "userPromptSubmitted" | "userPromptSubmit" => "UserPromptSubmit",
        "preToolUse" => "PreToolUse",
        "postToolUse" => "PostToolUse",
        "postToolUseFailure" => "PostToolUseFailure",
        "subagentStart" => "SubagentStart",
        "subagentStop" => "SubagentStop",
        "preCompact" => "PreCompact",
        "agentStop" | "stop" => "Stop",
        "errorOccurred" => "ErrorOccurred",
        "permissionRequest" => "PermissionRequest",
        "notification" => "Notification",
        other => other,
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Number(value)) => value.as_f64() != Some(0.0),
        _ => true,
    }
}

fn resolve_event(event: Option<&str>, payload: &Map<String, Value>) -> Option<String> {
    let explicit = event.map(str::to_string).or_else(|| {
        tool::read_first_string(
            payload,
            &["hook_event_name", "hookEventName", "hook_type", "hookType"],
        )
    });
    if explicit.as_ref().is_some_and(|event| !event.is_empty()) {
        return explicit;
    }
    let inferred = if tool::read_first_string(payload, &["initial_prompt", "initialPrompt"])
        .is_some()
    {
        "SessionStart"
    } else if tool::read_string(payload, "prompt").is_some() {
        "UserPromptSubmit"
    } else if tool::read_first_string(payload, &["notification_type", "notificationType"]).is_some()
    {
        "Notification"
    } else if tool::read_first_string(
        payload,
        &[
            "transcript_path",
            "transcriptPath",
            "stop_reason",
            "stopReason",
        ],
    )
    .is_some()
    {
        "Stop"
    } else if truthy(payload.get("error"))
        || tool::read_first_string(payload, &["error_context", "errorContext"]).is_some()
    {
        "ErrorOccurred"
    } else if payload.get("toolCalls").is_some_and(Value::is_array)
        || tool::read_first_string(payload, &["tool_name", "toolName", "name"]).is_some()
    {
        if ["tool_result", "toolResult", "tool_response", "toolResponse"]
            .iter()
            .any(|key| truthy(payload.get(*key)))
        {
            "PostToolUse"
        } else {
            "PreToolUse"
        }
    } else {
        return explicit;
    };
    Some(inferred.to_string())
}

pub fn normalize_copilot_event(
    state: &mut HookListenerState,
    event: Option<&str>,
    prompt: &str,
    pane: &str,
    payload: &Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let resolved = resolve_event(event, payload)?;
    let event = normalize_event_name(&resolved);
    let notification = tool::read_first_string(payload, &["notification_type", "notificationType"]);
    let blocking_notification = event == "Notification"
        && matches!(
            notification.as_deref(),
            Some("permission_prompt" | "elicitation_dialog")
        );
    let update = extract_fields(event, payload);
    let ask = matches!(event, "PreToolUse" | "PermissionRequest")
        && is_ask_user(update.tool_name.as_deref());
    let state_name = match event {
        "SessionStart" | "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" => "working",
        _ if blocking_notification || ask => "blocked",
        "PreToolUse" | "PermissionRequest" => "working",
        "Stop" | "SessionEnd" => "done",
        "ErrorOccurred" => {
            if payload.get("recoverable").and_then(Value::as_bool) == Some(true) {
                "working"
            } else {
                "done"
            }
        }
        _ => return None,
    };
    normalize_cached_status(
        state,
        pane,
        if event == "Notification" { "" } else { prompt },
        "copilot",
        state_name,
        matches!(event, "SessionStart" | "UserPromptSubmit"),
        update,
    )
}

fn parsed_input(value: &Value) -> Value {
    value
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| value.clone())
}

fn extract_fields(event: &str, payload: &Map<String, Value>) -> tool::ToolSnapshot {
    let mut update = tool::ToolSnapshot::default();
    if matches!(event, "PostToolUseFailure" | "ErrorOccurred") {
        update = tool::clear_active_tool_fields_update();
    } else if matches!(event, "PreToolUse" | "PostToolUse" | "PermissionRequest") {
        let call = payload
            .get("toolCalls")
            .and_then(Value::as_array)
            .and_then(|calls| calls.first())
            .and_then(Value::as_object);
        let call_name =
            call.and_then(|call| tool::read_first_string(call, &["name", "toolName", "tool_name"]));
        let call_input = call
            .and_then(|call| {
                call.get("args")
                    .filter(|value| !value.is_null())
                    .or_else(|| call.get("arguments"))
            })
            .map(parsed_input);
        let name =
            tool::read_first_string(payload, &["tool_name", "toolName", "name"]).or(call_name);
        let keys = ["tool_input", "toolInput", "toolArgs", "input", "arguments"];
        let input = keys
            .iter()
            .find_map(|key| tool::derive_tool_input_preview(name.as_deref(), payload.get(*key)))
            .or_else(|| tool::derive_tool_input_preview(name.as_deref(), call_input.as_ref()));
        let answer = if is_ask_user(name.as_deref()) {
            input.clone()
        } else {
            None
        };
        update = tool::tool_update(
            name,
            input,
            None,
            tool::has_any_own_field(payload, &keys) || call_input.is_some(),
        );
        update.last_assistant_message = answer;
    }
    if matches!(
        event,
        "PostToolUse" | "PostToolUseFailure" | "ErrorOccurred"
    ) {
        let response = ["tool_result", "toolResult", "tool_response", "toolResponse"]
            .iter()
            .find_map(|key| tool::extract_tool_response_text(payload.get(*key)))
            .or_else(|| {
                (event != "PostToolUse")
                    .then(|| {
                        tool::read_first_string(
                            payload,
                            &["error_message", "errorMessage", "error", "message"],
                        )
                    })
                    .flatten()
            });
        if response.is_some() {
            update.last_assistant_message = response;
        }
    }
    if event == "Notification"
        && matches!(
            tool::read_first_string(payload, &["notification_type", "notificationType"]).as_deref(),
            Some("permission_prompt" | "elicitation_dialog")
        )
    {
        update.last_assistant_message =
            tool::read_first_string(payload, &["message", "body", "text", "title"]);
    }
    if event == "Stop" {
        update.last_assistant_message = tool::read_first_string(
            payload,
            &["last_assistant_message", "lastAssistantMessage", "message"],
        );
        update.clear_last_assistant_message = update.last_assistant_message.is_none();
    }
    update
}
