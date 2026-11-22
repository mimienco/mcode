//! Field normalization — Rust port of `src/shared/agent-status-field-normalization.ts`,
//! `normalizeAgentStatusObject` (`agent-status-types.ts:395-430`), and
//! subagent-snapshot normalization (`:312-358`).

use serde_json::Value;

use crate::types::{
    AgentStatusState, AgentSubagentSnapshot, AgentSubagentState, ParsedAgentStatusPayload,
};

pub const AGENT_STATUS_MAX_FIELD_LENGTH: usize = 200;
pub const AGENT_TYPE_MAX_LENGTH: usize = 40;
pub const AGENT_MODEL_MAX_LENGTH: usize = 120;
pub const TOOL_NAME_MAX_LENGTH: usize = 60;
pub const TOOL_INPUT_MAX_LENGTH: usize = 160;
pub const ASSISTANT_MESSAGE_MAX_LENGTH: usize = 8000;
pub const INTERACTIVE_PROMPT_MAX_LENGTH: usize = 16000;
pub const AGENT_STATUS_MAX_SUBAGENTS: usize = 32;
pub const AGENT_SUBAGENT_ID_MAX_LENGTH: usize = 64;

// Why: hook prompt/tool fields are previews. Bound the source scan before
// folding line breaks so paste-sized status text cannot run a full regex
// replacement just to keep a small dashboard label.
const SINGLE_LINE_FIELD_SCAN_MULTIPLIER: usize = 8;
const SINGLE_LINE_FIELD_SCAN_OVERHEAD: usize = 64;

fn is_ecma_trim_whitespace(code: u32) -> bool {
    matches!(code,
        0x20 | 0x09..=0x0d | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029
        | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

pub(crate) fn trim_ecma(value: &str) -> &str {
    value.trim_matches(|ch: char| is_ecma_trim_whitespace(ch as u32))
}

fn is_single_line_separator(code: u32) -> bool {
    matches!(code, 0x0d | 0x0a | 0x2028 | 0x2029)
}

fn decode_complete_utf16(mut units: Vec<u16>) -> String {
    if units
        .last()
        .is_some_and(|unit| (0xd800..=0xdbff).contains(unit))
    {
        units.pop();
    }
    String::from_utf16(&units).expect("input was valid UTF-8 and trailing surrogate was removed")
}

pub(crate) fn truncate_utf16(value: &str, max_length: usize) -> String {
    decode_complete_utf16(value.encode_utf16().take(max_length).collect())
}

fn single_line_preview(value: &str, max_length: usize) -> String {
    let scan_limit = max_length
        .saturating_mul(SINGLE_LINE_FIELD_SCAN_MULTIPLIER)
        .saturating_add(SINGLE_LINE_FIELD_SCAN_OVERHEAD);
    let codes: Vec<u16> = value
        .encode_utf16()
        .take(scan_limit.saturating_add(1))
        .collect();
    let scan_end = codes.len().min(scan_limit);
    let mut index = 0;
    while index < scan_end && is_ecma_trim_whitespace(u32::from(codes[index])) {
        index += 1;
    }
    let mut normalized = Vec::new();
    let mut line_separator_run = false;
    while index < scan_end && normalized.len() < max_length {
        let code = codes[index];
        if is_single_line_separator(u32::from(code)) {
            // Fold \r\n into one separator.
            if code == 0x0d && index + 1 < codes.len() && codes[index + 1] == 0x0a {
                index += 1;
            }
            if !line_separator_run {
                normalized.push(0x20);
            }
            line_separator_run = true;
            index += 1;
            continue;
        }
        normalized.push(code);
        line_separator_run = false;
        index += 1;
    }
    if normalized.len() < max_length {
        while normalized
            .last()
            .is_some_and(|code| is_ecma_trim_whitespace(u32::from(*code)))
        {
            normalized.pop();
        }
    }
    decode_complete_utf16(normalized)
}

/// `agent-status-field-normalization.ts:normalizeMultilineField` — preserve
/// paragraph structure; `\r\n`→`\n`, U+2028/2029→`\n`, blank-line runs capped.
fn multiline_field(value: &str, max_length: usize) -> String {
    let trimmed = value.trim_matches(|ch: char| is_ecma_trim_whitespace(ch as u32));
    let mut normalized = String::new();
    let mut budget = max_length;
    let mut newline_run = 0usize;
    let mut chars = trimmed.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if budget == 0 {
            break;
        }
        let code = ch as u32;
        if matches!(code, 0x0d | 0x0a | 0x2028 | 0x2029) {
            chars.next();
            if code == 0x0d && chars.peek().map(|&n| n as u32) == Some(0x0a) {
                chars.next();
            }
            if newline_run < 2 {
                normalized.push('\n');
                budget -= 1;
            }
            newline_run += 1;
            continue;
        }
        chars.next();
        if budget < ch.len_utf16() {
            break;
        }
        normalized.push(ch);
        budget -= ch.len_utf16();
        newline_run = 0;
        if budget == 0 {
            break;
        }
    }
    normalized
}

pub fn normalize_optional_field(value: Option<&str>, max_length: usize) -> Option<String> {
    let value = value?;
    let normalized = single_line_preview(value, max_length);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub fn normalize_optional_multiline_field(
    value: Option<&str>,
    max_length: usize,
) -> Option<String> {
    let value = value?;
    let normalized = multiline_field(value, max_length);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// interactivePrompt carries raw JSON; never trim/collapse — only cap length
/// and drop empties (`agent-status-field-normalization.ts:185-194`).
pub fn normalize_interactive_prompt_field(
    value: Option<&str>,
    max_length: usize,
) -> Option<String> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    let truncated = truncate_utf16(value, max_length);
    (!truncated.is_empty()).then_some(truncated)
}

/// `agent-status-field-normalization.ts:216-221` — keep a lead turn's end
/// time only on the gated `working` row and that turn's later all-clear `done`.
pub fn normalize_turn_completed_at(value: Option<i64>, state: AgentStatusState) -> Option<i64> {
    let value = value?;
    match state {
        AgentStatusState::Working | AgentStatusState::Done => Some(value),
        _ => None,
    }
}

// ─── MCode dispatch preamble compaction (mcode-dispatch-status-prompt.ts) ───

pub const MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX: &str =
    "You are working inside MCode, a multi-agent IDE.";
pub const MCODE_DISPATCH_STATUS_TASK_MARKER: &str = "=== TASK ===";
const MCODE_DISPATCH_STATUS_TASK_ID_MARKER: &str = "Your task ID is:";
const MCODE_DISPATCH_STATUS_SOURCE_SCAN_LIMIT: usize = 24_576;

pub fn is_mcode_dispatch_status_prompt(value: &str) -> bool {
    let bounded = truncate_utf16(value, MCODE_DISPATCH_STATUS_SOURCE_SCAN_LIMIT);
    let trimmed = bounded.trim_start_matches(|ch: char| is_ecma_trim_whitespace(ch as u32));
    trimmed.starts_with(MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX)
}

fn find_dispatch_task_marker_index(value: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(marker_index) = value[search_from..].find(MCODE_DISPATCH_STATUS_TASK_MARKER) {
        let marker_index = search_from + marker_index;
        let marker_end = marker_index + MCODE_DISPATCH_STATUS_TASK_MARKER.len();
        let starts_line =
            marker_index == 0 || matches!(value.as_bytes()[marker_index - 1], b'\n' | b'\r');
        let ends_line =
            marker_end == value.len() || matches!(value.as_bytes()[marker_end], b'\n' | b'\r');
        if starts_line && ends_line {
            return Some(marker_index);
        }
        search_from = marker_end;
    }
    if value.contains('\n') || value.contains('\r') {
        None
    } else {
        value.find(MCODE_DISPATCH_STATUS_TASK_MARKER)
    }
}

/// Collapse a multi-KB dispatch preamble into a 200-char single-line preview
/// still carrying `<prefix> Your task ID is: <id> === TASK === <task body>`.
fn compact_dispatch_prompt(value: &str, max_length: usize) -> String {
    let bounded = truncate_utf16(value, MCODE_DISPATCH_STATUS_SOURCE_SCAN_LIMIT);
    let scan = bounded.trim_start_matches(|ch: char| is_ecma_trim_whitespace(ch as u32));
    let mut task_id = String::new();
    if let Some(id_marker_index) = scan.find(MCODE_DISPATCH_STATUS_TASK_ID_MARKER) {
        let after_id =
            scan[id_marker_index + MCODE_DISPATCH_STATUS_TASK_ID_MARKER.len()..].trim_start();
        let id_end = after_id.find(char::is_whitespace).unwrap_or(after_id.len());
        task_id = after_id[..id_end].trim().to_string();
    }
    let mut task_body = String::new();
    if let Some(task_marker_index) = find_dispatch_task_marker_index(scan) {
        let body = &scan[task_marker_index + MCODE_DISPATCH_STATUS_TASK_MARKER.len()..];
        for line in body.lines() {
            let preview: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if !preview.is_empty() {
                task_body = preview;
                break;
            }
        }
    }
    let mut compact = String::from(MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX);
    if !task_id.is_empty() {
        compact.push(' ');
        compact.push_str(MCODE_DISPATCH_STATUS_TASK_ID_MARKER);
        compact.push(' ');
        compact.push_str(&task_id);
    }
    if !task_body.is_empty() {
        compact.push(' ');
        compact.push_str(MCODE_DISPATCH_STATUS_TASK_MARKER);
        compact.push(' ');
        compact.push_str(&task_body);
    }
    single_line_preview(&compact, max_length)
}

pub fn normalize_prompt_field(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if is_mcode_dispatch_status_prompt(value) {
        return compact_dispatch_prompt(value, AGENT_STATUS_MAX_FIELD_LENGTH);
    }
    single_line_preview(value, AGENT_STATUS_MAX_FIELD_LENGTH)
}

// ─── ParsedAgentStatusPayload normalization ────────────────────────────────

fn normalize_subagent_snapshot(value: &Value) -> Option<AgentSubagentSnapshot> {
    let obj = value.as_object()?;
    let id = obj.get("id")?.as_str()?.trim();
    if id.is_empty() || id.encode_utf16().count() > AGENT_SUBAGENT_ID_MAX_LENGTH {
        return None;
    }
    let state = match obj.get("state").and_then(Value::as_str)? {
        "working" => AgentSubagentState::Working,
        "blocked" => AgentSubagentState::Blocked,
        "waiting" => AgentSubagentState::Waiting,
        "idle" => AgentSubagentState::Idle,
        _ => return None,
    };
    Some(AgentSubagentSnapshot {
        id: id.to_string(),
        state,
        started_at: obj
            .get("startedAt")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v as i64)
            .unwrap_or(0),
        agent_type: normalize_optional_field(
            obj.get("agentType").and_then(Value::as_str),
            AGENT_TYPE_MAX_LENGTH,
        ),
        model: normalize_optional_field(
            obj.get("model").and_then(Value::as_str),
            AGENT_MODEL_MAX_LENGTH,
        ),
        description: normalize_optional_field(
            obj.get("description").and_then(Value::as_str),
            TOOL_INPUT_MAX_LENGTH,
        ),
    })
}

pub fn normalize_subagents_field(value: Option<&Vec<Value>>) -> Option<Vec<AgentSubagentSnapshot>> {
    let items = value?;
    if items.is_empty() {
        return None;
    }
    let mut normalized = Vec::new();
    for item in items {
        if let Some(snapshot) = normalize_subagent_snapshot(item) {
            normalized.push(snapshot);
            if normalized.len() >= AGENT_STATUS_MAX_SUBAGENTS {
                break;
            }
        }
    }
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn parse_state(state: &str) -> Option<AgentStatusState> {
    match state {
        "working" => Some(AgentStatusState::Working),
        "blocked" => Some(AgentStatusState::Blocked),
        "waiting" => Some(AgentStatusState::Waiting),
        "done" => Some(AgentStatusState::Done),
        _ => None,
    }
}

/// `normalizeAgentStatusObject` (`agent-status-types.ts:395-430`) — field caps,
/// state-gated `interrupted`/`sessionBoundary`/`turnCompletedAt`.
pub fn normalize_agent_status_object(
    record: &serde_json::Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let state_str = record.get("state").and_then(Value::as_str)?;
    let state = parse_state(state_str)?;
    let interrupted = record.get("interrupted").and_then(Value::as_bool) == Some(true)
        && state == AgentStatusState::Done;
    let session_boundary = record.get("sessionBoundary").and_then(Value::as_bool) == Some(true)
        && state == AgentStatusState::Done;
    Some(ParsedAgentStatusPayload {
        state,
        prompt: normalize_prompt_field(record.get("prompt").and_then(Value::as_str)),
        agent_type: normalize_optional_field(
            record.get("agentType").and_then(Value::as_str),
            AGENT_TYPE_MAX_LENGTH,
        ),
        model: normalize_optional_field(
            record.get("model").and_then(Value::as_str),
            AGENT_MODEL_MAX_LENGTH,
        ),
        tool_name: normalize_optional_field(
            record.get("toolName").and_then(Value::as_str),
            TOOL_NAME_MAX_LENGTH,
        ),
        tool_input: normalize_optional_field(
            record.get("toolInput").and_then(Value::as_str),
            TOOL_INPUT_MAX_LENGTH,
        ),
        interactive_prompt: normalize_interactive_prompt_field(
            record.get("interactivePrompt").and_then(Value::as_str),
            INTERACTIVE_PROMPT_MAX_LENGTH,
        ),
        last_assistant_message: normalize_optional_multiline_field(
            record.get("lastAssistantMessage").and_then(Value::as_str),
            ASSISTANT_MESSAGE_MAX_LENGTH,
        ),
        interrupted,
        session_boundary,
        turn_completed_at: normalize_turn_completed_at(
            record.get("turnCompletedAt").and_then(Value::as_i64),
            state,
        ),
        subagents: normalize_subagents_field(record.get("subagents").and_then(Value::as_array)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn single_line_folds_newlines_and_caps() {
        assert_eq!(single_line_preview("a\r\nb\nc", 40), "a b c");
        let long = "x".repeat(300);
        assert_eq!(single_line_preview(&long, 200).chars().count(), 200);
    }

    #[test]
    fn multiline_preserves_structure_and_caps_blank_runs() {
        assert_eq!(multiline_field("a\n\n\n\nb", 40), "a\n\nb");
        assert_eq!(multiline_field("a\r\nb\u{2029}c", 40), "a\nb\nc");
    }

    #[test]
    fn interrupted_only_on_done() {
        let payload = normalize_agent_status_object(
            &serde_json::to_value(json!({"state":"working","prompt":"p","interrupted":true}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(!payload.interrupted);
        let payload = normalize_agent_status_object(
            &serde_json::to_value(json!({"state":"done","prompt":"p","interrupted":true}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(payload.interrupted);
    }

    #[test]
    fn dispatch_prompt_compacts() {
        let preamble = format!(
            "{} filler filler\n{}\nbuild the thing",
            MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX, MCODE_DISPATCH_STATUS_TASK_MARKER
        );
        assert!(is_mcode_dispatch_status_prompt(&preamble));
        let compacted = normalize_prompt_field(Some(&preamble));
        assert!(compacted.contains(MCODE_DISPATCH_STATUS_TASK_MARKER));
        assert!(compacted.chars().count() <= AGENT_STATUS_MAX_FIELD_LENGTH);
    }

    #[test]
    fn turn_completed_at_state_gate() {
        assert_eq!(
            normalize_turn_completed_at(Some(5), AgentStatusState::Waiting),
            None
        );
        assert_eq!(
            normalize_turn_completed_at(Some(5), AgentStatusState::Done),
            Some(5)
        );
    }

    #[test]
    fn field_caps_match_javascript_utf16_without_dropping_following_text() {
        assert_eq!(single_line_preview("ééé", 2), "éé");
        assert_eq!(single_line_preview("😀ab", 3), "😀a");
        assert_eq!(single_line_preview("a😀b", 2), "a");
        assert_eq!(multiline_field("😀ab", 3), "😀a");
        assert_eq!(multiline_field("a😀b", 2), "a");
        assert_eq!(
            normalize_interactive_prompt_field(Some("😀ab"), 3).as_deref(),
            Some("😀a")
        );
    }

    #[test]
    fn multiline_newlines_consume_the_same_field_budget_as_text() {
        assert_eq!(multiline_field("a\n\nb\nc", 4), "a\n\nb");
        assert_eq!(multiline_field("a\r\n\r\nb", 2), "a\n");
        assert_eq!(multiline_field("a\u{2028}\u{2029}b", 3), "a\n\n");
        assert_eq!(multiline_field("a\n", 0), "");
    }

    #[test]
    fn ecmascript_whitespace_and_source_scan_limit_match_electron() {
        assert_eq!(
            single_line_preview("\u{feff}message\u{feff}", 40),
            "message"
        );
        assert_eq!(multiline_field("\u{feff}message\u{feff}", 40), "message");
        assert_eq!(
            single_line_preview(&format!("{}ok", " ".repeat(1300)), 200),
            "ok"
        );
        assert_eq!(
            single_line_preview(&format!("{}hidden", " ".repeat(1664)), 200),
            ""
        );
    }

    #[test]
    fn dispatch_scan_is_bounded_and_never_slices_inside_utf8() {
        let prompt = format!(
            "{}{}",
            MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX,
            "😀".repeat(30_000)
        );
        assert_eq!(
            normalize_prompt_field(Some(&prompt)),
            MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX
        );
        let padded = format!(
            "{}{}",
            " ".repeat(MCODE_DISPATCH_STATUS_SOURCE_SCAN_LIMIT),
            MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX
        );
        assert!(!is_mcode_dispatch_status_prompt(&padded));
        let outside = format!(
            "{}{}\n=== TASK ===\nignored",
            MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX,
            "x".repeat(MCODE_DISPATCH_STATUS_SOURCE_SCAN_LIMIT)
        );
        assert_eq!(
            normalize_prompt_field(Some(&outside)),
            MCODE_DISPATCH_STATUS_PREAMBLE_PREFIX
        );
    }
}
