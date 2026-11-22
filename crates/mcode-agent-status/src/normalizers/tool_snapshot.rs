//! Tool-snapshot state machine — Rust port of `agent-hook-listener.ts:670-900`
//! (`ToolSnapshot`, `resolveToolState`, input-preview derivation,
//! `deriveInteractivePrompt`, `extractToolResponseText`).

use serde_json::Value;

use crate::normalizers::fields::{
    TOOL_INPUT_MAX_LENGTH, TOOL_NAME_MAX_LENGTH, normalize_interactive_prompt_field,
    normalize_optional_field,
};
use crate::normalizers::harness_text::normalize_hook_event_name;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolSnapshot {
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    /// Full JSON of an AskUserQuestion tool input; set only on its own event
    /// and NOT inherited so no stale prompt lingers.
    pub interactive_prompt: Option<String>,
    pub has_tool_update: bool,
    pub has_tool_input_field: bool,
    pub last_assistant_message: Option<String>,
    pub clear_last_assistant_message: bool,
}

impl ToolSnapshot {
    pub fn is_empty_update(&self) -> bool {
        !self.has_tool_update && self.last_assistant_message.is_none()
    }
}

/// `TOOL_INPUT_KEYS_BY_TOOL` (`agent-hook-listener.ts:719-791`).
fn tool_input_keys(tool_name: &str) -> Option<&'static [&'static str]> {
    Some(match tool_name {
        "Read" | "Write" | "Create" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            &["file_path", "filePath", "path"]
        }
        "Execute" | "Bash" | "bash" | "powershell" | "terminal" => &["command"],
        "Glob" | "glob" | "Grep" | "grep" => &["pattern"],
        "WebFetch" | "FetchUrl" | "web_fetch" | "fetch_content" | "browser_navigate" => &["url"],
        "WebSearch" | "google_web_search" | "web_search" | "search_web" => &["query"],
        "read_file" | "write_file" | "edit_file" | "replace" => &["file_path", "path"],
        "read_many_files" => &["file_path", "paths", "path"],
        "run_shell_command" => &["command"],
        "run_command" => &["CommandLine", "command", "cmd"],
        "search_file_content" => &["pattern"],
        "exec_command" | "shell_command" => &["cmd", "command"],
        "run_terminal_cmd" | "run_terminal_command" => &["command"],
        "search_replace" => &["file_path", "path", "filePath"],
        "write_to_file" | "replace_file_content" | "multi_replace_file_content" => {
            &["TargetFile", "path", "file_path"]
        }
        "execute_code" => &["code", "command", "cmd"],
        "apply_patch" | "patch" | "view_image" | "view" => &["path", "file_path"],
        "AskUser" | "ask_user" => &["question", "prompt", "message"],
        "AskUserQuestion" | "ask_user_question" => &["questions", "question", "prompt", "message"],
        "create" | "read" | "write" | "edit" => &["path", "file_path"],
        "search_files" => &["query", "pattern", "path"],
        "browser_click" => &["target", "selector", "text"],
        "browser_type" => &["text", "target", "selector"],
        "session_search" => &["query"],
        "skill_manage" => &["action", "name", "file_path"],
        "delegate_task" => &["task", "prompt", "description"],
        "view_file" => &["AbsolutePath", "path", "file_path"],
        "list_dir" => &["DirectoryPath", "path"],
        "find_by_name" => &["SearchDirectory", "Pattern", "query"],
        "grep_search" => &["SearchPath", "Query", "query", "pattern"],
        "read_url_content" => &["Url", "url"],
        "manage_task" => &["TaskId", "Action"],
        "schedule" => &["Prompt", "DurationSeconds", "CronExpression"],
        "ask_question" => &["question", "questions"],
        "ask_permission" => &["Action", "Target", "Reason"],
        "spawn_subagent" => &["prompt", "description", "subagent_type"],
        "open_page" => &["url"],
        _ => return None,
    })
}

/// `FALLBACK_TOOL_INPUT_KEYS` (`agent-hook-listener.ts:793-817`).
const FALLBACK_TOOL_INPUT_KEYS: &[&str] = &[
    "command",
    "cmd",
    "code",
    "query",
    "pattern",
    "url",
    "path",
    "file_path",
    "filePath",
    "target",
    "selector",
    "text",
    "action",
    "name",
    "description",
    "CommandLine",
    "AbsolutePath",
    "TargetFile",
    "DirectoryPath",
    "SearchPath",
    "Query",
    "Url",
    "Prompt",
];

fn first_string_in(record: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = record.get(*key).and_then(Value::as_str)
            && !value.is_empty()
        {
            return Some(value.to_string());
        }
    }
    None
}

/// `deriveFallbackToolInputPreview` (`agent-hook-listener.ts:846-861`) — why:
/// providers with many tool names fall back to obvious arg fields so a new
/// name still shows a value, not a blank row.
pub fn derive_fallback_tool_input_preview(tool_input: Option<&Value>) -> Option<String> {
    match tool_input {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Object(record)) => first_string_in(record, FALLBACK_TOOL_INPUT_KEYS),
        _ => None,
    }
}

pub fn read_string(record: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn read_first_string(record: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| read_string(record, key))
}

pub fn has_own_field(record: &serde_json::Map<String, Value>, key: &str) -> bool {
    record.contains_key(key)
}

pub fn has_any_own_field(record: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter().any(|key| has_own_field(record, key))
}

pub fn derive_tool_input_preview(
    tool_name: Option<&str>,
    tool_input: Option<&Value>,
) -> Option<String> {
    match tool_input {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Object(record)) => {
            let tool_name = tool_name?;
            let keys = tool_input_keys(tool_name)?;
            for key in keys {
                if let Some(value) = record.get(*key).and_then(Value::as_str)
                    && !value.trim().is_empty()
                {
                    return Some(value.to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// `summarizeApprovalInput` (`agent-hook-listener.ts:902-916`).
fn summarize_approval_input(tool_input: Option<&Value>) -> String {
    if let Some(Value::Object(record)) = tool_input {
        for key in ["command", "file_path", "path", "url", "pattern"] {
            if let Some(direct) = record.get(key).and_then(Value::as_str)
                && !direct.is_empty()
            {
                return clip_200(direct);
            }
        }
    }
    match tool_input {
        Some(value) => clip_200(&value.to_string()),
        None => String::new(),
    }
}

fn clip_200(text: &str) -> String {
    if text.chars().count() > 200 {
        format!("{}\u{2026}", text.chars().take(200).collect::<String>())
    } else {
        text.to_string()
    }
}

/// `deriveInteractivePrompt` (`agent-hook-listener.ts:919-951`) — AskUserQuestion
/// → `{ questions }`; other tool on PermissionRequest → `{ approval }`.
pub fn derive_interactive_prompt(
    tool_name: Option<&str>,
    tool_input: Option<&Value>,
    event_name: Option<&str>,
) -> Option<String> {
    let normalized_event_name = normalize_hook_event_name(event_name.unwrap_or_default());
    let is_post_tool_event = normalized_event_name == "post_tool_use"
        || normalized_event_name == "post_tool_use_failure";
    if crate::normalizers::ask_user::is_ask_user_question_tool(tool_name)
        && !is_post_tool_event
        && let Some(tool_input) = tool_input
    {
        return Some(tool_input.to_string());
    }
    if event_name == Some("PermissionRequest") {
        let tool_name = tool_name?;
        return Some(
            serde_json::json!({
                "approval": { "tool": tool_name, "summary": summarize_approval_input(tool_input) }
            })
            .to_string(),
        );
    }
    None
}

/// `extractToolResponseText` (`agent-hook-listener.ts:980-1004`).
pub fn extract_tool_response_text(tool_response: Option<&Value>) -> Option<String> {
    match tool_response? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Object(record) => {
            if let Some(direct) =
                read_first_string(record, &["text_result_for_llm", "textResultForLlm", "text"])
            {
                return Some(direct);
            }
            if let Some(Value::Array(content)) = record.get("content") {
                for part in content {
                    if let Some(text) = part.get("text").and_then(Value::as_str)
                        && !text.trim().is_empty()
                    {
                        return Some(text.to_string());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// `toolUpdate` (`agent-hook-listener.ts:876-885`).
pub fn tool_update(
    tool_name: Option<String>,
    tool_input: Option<String>,
    interactive_prompt: Option<String>,
    has_tool_input_field: bool,
) -> ToolSnapshot {
    ToolSnapshot {
        tool_name,
        tool_input,
        interactive_prompt,
        has_tool_update: true,
        has_tool_input_field,
        ..ToolSnapshot::default()
    }
}

/// `clearActiveToolFieldsUpdate` (`agent-hook-listener.ts:888-893`).
pub fn clear_active_tool_fields_update() -> ToolSnapshot {
    tool_update(None, None, None, true)
}

/// `resolveToolState` (`agent-hook-listener.ts:681-717`) — the merge rules that
/// decide inheritance vs clearing of cached tool metadata.
pub fn resolve_tool_state(
    cache: &mut std::collections::HashMap<String, ToolSnapshot>,
    pane_key: &str,
    update: ToolSnapshot,
    reset_on_new_turn: bool,
) -> ToolSnapshot {
    if reset_on_new_turn {
        cache.remove(pane_key);
    }
    let previous = cache.get(pane_key).cloned().unwrap_or_default();
    let clears_unpreviewable_input = update.has_tool_input_field && update.tool_input.is_none();
    let clears_unidentified_tool =
        update.has_tool_update && update.tool_name.is_none() && update.has_tool_input_field;
    let tool_name = if clears_unidentified_tool {
        None
    } else {
        update
            .tool_name
            .clone()
            .or_else(|| previous.tool_name.clone())
    };
    let tool_input = if clears_unpreviewable_input
        || (update.tool_name.is_some()
            && previous.tool_name.is_some()
            && update.tool_name != previous.tool_name
            && update.tool_input.is_none())
    {
        None
    } else {
        update
            .tool_input
            .clone()
            .or_else(|| previous.tool_input.clone())
    };
    let merged = ToolSnapshot {
        tool_name: tool_name.and_then(|n| normalize_optional_field(Some(&n), TOOL_NAME_MAX_LENGTH)),
        tool_input: tool_input
            .and_then(|v| normalize_optional_field(Some(&v), TOOL_INPUT_MAX_LENGTH)),
        // Why: don't inherit previous.interactivePrompt — valid only for its one
        // AskUserQuestion event; carrying it forward leaves a stale live card.
        interactive_prompt: update
            .interactive_prompt
            .as_deref()
            .and_then(|p| normalize_interactive_prompt_field(Some(p), 16_000)),
        has_tool_update: true,
        has_tool_input_field: false,
        last_assistant_message: if update.clear_last_assistant_message {
            None
        } else {
            update
                .last_assistant_message
                .clone()
                .or_else(|| previous.last_assistant_message.clone())
        },
        clear_last_assistant_message: false,
    };
    cache.insert(pane_key.to_string(), merged.clone());
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cache() -> std::collections::HashMap<String, ToolSnapshot> {
        std::collections::HashMap::new()
    }

    #[test]
    fn pre_tool_use_merges_and_next_tool_clears_stale_input() {
        let mut cache = cache();
        let first = resolve_tool_state(
            &mut cache,
            "p",
            tool_update(Some("Bash".into()), Some("ls".into()), None, true),
            false,
        );
        assert_eq!(first.tool_name.as_deref(), Some("Bash"));
        let second = resolve_tool_state(
            &mut cache,
            "p",
            tool_update(Some("Read".into()), None, None, true),
            false,
        );
        assert_eq!(second.tool_name.as_deref(), Some("Read"));
        assert_eq!(second.tool_input, None);
    }

    #[test]
    fn interactive_prompt_not_inherited() {
        let mut cache = cache();
        resolve_tool_state(
            &mut cache,
            "p",
            tool_update(
                Some("AskUserQuestion".into()),
                None,
                Some("{\"questions\":[]}".into()),
                true,
            ),
            false,
        );
        let next = resolve_tool_state(
            &mut cache,
            "p",
            tool_update(Some("PostToolUse-ish".into()), None, None, true),
            false,
        );
        assert_eq!(next.interactive_prompt, None);
    }

    #[test]
    fn failure_clears_active_tool() {
        let mut cache = cache();
        resolve_tool_state(
            &mut cache,
            "p",
            tool_update(Some("Bash".into()), Some("x".into()), None, true),
            false,
        );
        let cleared = resolve_tool_state(&mut cache, "p", clear_active_tool_fields_update(), false);
        assert_eq!(cleared.tool_name, None);
        assert_eq!(cleared.tool_input, None);
    }

    #[test]
    fn ask_user_question_interactive_prompt_is_raw_json() {
        let input = json!({"questions": [{"question": "Q?"}]});
        let derived =
            derive_interactive_prompt(Some("AskUserQuestion"), Some(&input), Some("PreToolUse"));
        assert_eq!(derived.as_deref(), Some(input.to_string().as_str()));
    }

    #[test]
    fn approval_envelope_for_permission_request() {
        let derived = derive_interactive_prompt(
            Some("Bash"),
            Some(&json!({"command": "rm -rf /"})),
            Some("PermissionRequest"),
        );
        let parsed: Value = serde_json::from_str(&derived.unwrap()).unwrap();
        assert_eq!(parsed["approval"]["tool"], "Bash");
    }
}
