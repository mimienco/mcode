//! Shared question/intent predicates — Rust port of
//! `src/shared/agent-question-answered-intent.ts:20-24` and
//! `src/shared/agent-interrupt-intent.ts:16-18`.

/// True for the ask-the-user-a-question tool across agents: Claude's
/// `AskUserQuestion`, grok/Pi's `ask_user_question`, and Codex ≥0.145's
/// `request_user_input` (same questions/options input shape).
pub fn is_ask_user_question_tool(tool_name: Option<&str>) -> bool {
    let normalized = tool_name
        .map(|name| {
            name.chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .unwrap_or_default();
    normalized == "askuserquestion" || normalized == "requestuserinput"
}

/// `agent-interrupt-intent.ts:16-18`.
pub fn is_agent_interrupt_input_intent(intent: &str) -> bool {
    intent == "plain-escape" || intent == "ctrl-c"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_user_tool_variants() {
        assert!(is_ask_user_question_tool(Some("AskUserQuestion")));
        assert!(is_ask_user_question_tool(Some("ask_user_question")));
        assert!(is_ask_user_question_tool(Some("request_user_input")));
        assert!(is_ask_user_question_tool(Some("RequestUserInput")));
        assert!(!is_ask_user_question_tool(Some("Bash")));
        assert!(!is_ask_user_question_tool(None));
    }

    #[test]
    fn interrupt_intents() {
        assert!(is_agent_interrupt_input_intent("plain-escape"));
        assert!(is_agent_interrupt_input_intent("ctrl-c"));
        assert!(!is_agent_interrupt_input_intent("escape"));
    }
}
