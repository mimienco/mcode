//! Remaining providers' direct-hook contracts. Filesystem fallbacks are separate.

use mcode_agent_status::listener_state::{HookListenerState, clear_pane_cache_state};
use mcode_agent_status::normalizers::{NormalizeError, normalize_hook_payload};
use mcode_agent_status::types::{
    AgentHookEventPayload, AgentHookSource as Source, AgentStatusState as State,
};
use serde_json::{Value, json};

const PANE: &str = "tab-1:3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a";

fn hook(
    state: &mut HookListenerState,
    source: Source,
    payload: Value,
) -> Option<AgentHookEventPayload> {
    let result = normalize_hook_payload(
        state,
        source,
        json!({"paneKey": PANE}).as_object().unwrap(),
        Some(&payload.to_string()),
        "production",
    );
    match result {
        Ok(event) => Some(event),
        Err(NormalizeError::NoPayload) => None,
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn hermes_approval_uses_fallback_description_and_resumes_work() {
    let mut state = HookListenerState::default();
    hook(
        &mut state,
        Source::Hermes,
        json!({"hook_event_name": "pre_llm_call", "prompt": "review"}),
    );
    let waiting = hook(
        &mut state,
        Source::Hermes,
        json!({"hook_event_name": "pre_approval_request", "description": "Run the tests"}),
    )
    .unwrap();
    assert_eq!(waiting.payload.state, State::Waiting);
    assert_eq!(waiting.payload.tool_name.as_deref(), Some("approval"));
    assert_eq!(waiting.payload.tool_input.as_deref(), Some("Run the tests"));
    let resumed = hook(
        &mut state,
        Source::Hermes,
        json!({"hook_event_name": "post_approval_response"}),
    )
    .unwrap();
    assert_eq!(resumed.payload.state, State::Working);
    let output = hook(&mut state, Source::Hermes, json!({"hook_event_name": "post_tool_call", "name": "plugin_tool", "args": {"query": "test"}, "result": {"text": "passed"}})).unwrap();
    assert_eq!(output.payload.tool_input.as_deref(), Some("test"));
    assert_eq!(
        output.payload.last_assistant_message.as_deref(),
        Some("passed")
    );
    let done = hook(
        &mut state,
        Source::Hermes,
        json!({"hook_event_name": "post_llm_call", "assistant_response": "Finished"}),
    )
    .unwrap();
    assert_eq!(done.payload.state, State::Done);
    assert_eq!(
        done.payload.last_assistant_message.as_deref(),
        Some("Finished")
    );
    let new = hook(
        &mut state,
        Source::Hermes,
        json!({"hook_event_name": "pre_llm_call"}),
    )
    .unwrap();
    assert!(new.payload.tool_name.is_none());
    assert!(new.payload.last_assistant_message.is_none());
}

#[test]
fn opencode_family_user_messages_have_interaction_keys_and_permissions_retire() {
    for source in [Source::Opencode, Source::MimoCode] {
        let mut state = HookListenerState::default();
        let user = hook(&mut state, source, json!({"hook_event_name": "MessagePart", "role": "user", "text": "review", "messageID": "message-1", "sessionID": "session-1"})).unwrap();
        assert!(user.has_explicit_prompt);
        assert_eq!(
            user.prompt_interaction_key,
            Some(format!("{}-message-message-1", source.as_str()))
        );
        assert_eq!(user.provider_session.unwrap().id, "session-1");
        let permission = hook(&mut state, source, json!({"hook_event_name": "PermissionRequest", "permission": "edit", "metadata": {"filepath": "src/main.ts", "diff": "large patch"}, "patterns": ["fallback"]})).unwrap();
        assert_eq!(permission.payload.state, State::Waiting);
        assert_eq!(
            permission.payload.tool_input.as_deref(),
            Some("src/main.ts")
        );
        let answer = hook(
            &mut state,
            source,
            json!({"hook_event_name": "MessagePart", "role": "assistant", "text": "edited"}),
        )
        .unwrap();
        assert!(answer.payload.tool_name.is_none());
        assert!(answer.payload.tool_input.is_none());
        assert_eq!(answer.payload.prompt, "review");
        assert_eq!(
            answer.payload.last_assistant_message.as_deref(),
            Some("edited")
        );
        let idle = hook(
            &mut state,
            source,
            json!({"hook_event_name": "SessionIdle"}),
        )
        .unwrap();
        assert_eq!(idle.payload.state, State::Done);
    }
}

#[test]
fn opencode_question_strips_envelope_and_uses_explicit_tool_input_when_present() {
    let mut state = HookListenerState::default();
    let question = hook(&mut state, Source::Opencode, json!({"hook_event_name": "AskUserQuestion", "questions": [{"question": "Which?"}], "sessionID": "s"})).unwrap();
    let input: Value = serde_json::from_str(&question.payload.interactive_prompt.unwrap()).unwrap();
    assert!(input.get("hook_event_name").is_none());
    assert_eq!(input["questions"][0]["question"], "Which?");
    let explicit = hook(&mut state, Source::Opencode, json!({"hook_event_name": "AskUserQuestion", "questions": ["ignored"], "tool_input": {"questions": ["chosen"]}})).unwrap();
    let input: Value = serde_json::from_str(&explicit.payload.interactive_prompt.unwrap()).unwrap();
    assert_eq!(input, json!({"questions": ["chosen"]}));
}

#[test]
fn droid_idle_notifications_preserve_user_prompt_and_high_risk_tools_wait() {
    let mut state = HookListenerState::default();
    assert!(
        hook(
            &mut state,
            Source::Droid,
            json!({"hook_event_name": "SessionStart"})
        )
        .is_none()
    );
    hook(
        &mut state,
        Source::Droid,
        json!({"hook_event_name": "UserPromptSubmit", "prompt": "review"}),
    );
    for payload in [
        json!({"hook_event_name": "PreToolUse", "name": "AskUser", "input": {"question": "Choose?"}}),
        json!({"hook_event_name": "PreToolUse", "name": "Execute", "arguments": {"command": "deploy", "risk_level": " HIGH "}}),
    ] {
        assert_eq!(
            hook(&mut state, Source::Droid, payload)
                .unwrap()
                .payload
                .state,
            State::Waiting
        );
    }
    assert!(
        hook(
            &mut state,
            Source::Droid,
            json!({"hook_event_name": "Notification", "message": "task confirmed"})
        )
        .is_none()
    );
    let idle = hook(
        &mut state,
        Source::Droid,
        json!({"hook_event_name": "Notification", "message": "Waiting for your input"}),
    )
    .unwrap();
    assert_eq!(idle.payload.state, State::Done);
    assert_eq!(idle.payload.prompt, "review");
    assert!(!idle.has_explicit_prompt);
}

#[test]
fn command_code_ignores_status_message_as_prompt_and_extracts_direct_tool_output() {
    let mut state = HookListenerState::default();
    let event = hook(&mut state, Source::CommandCode, json!({"hook_event_name": "PreToolUse", "message": "status noise", "tool_display_name": "custom-tool", "tool_input": {"query": "needle"}})).unwrap();
    assert_eq!(event.payload.prompt, "");
    assert_eq!(event.payload.tool_input.as_deref(), Some("needle"));
    assert!(!event.has_explicit_prompt);
    let prompt = hook(
        &mut state,
        Source::CommandCode,
        json!({"hook_event_name": "PreToolUse", "prompt": "review"}),
    )
    .unwrap();
    assert!(prompt.has_explicit_prompt);
    let result = hook(&mut state, Source::CommandCode, json!({"hook_event_name": "PostToolUse", "tool_response": {"text_result_for_llm": "all passed"}})).unwrap();
    assert_eq!(
        result.payload.last_assistant_message.as_deref(),
        Some("all passed")
    );
    let stop = hook(
        &mut state,
        Source::CommandCode,
        json!({"hook_event_name": "Stop", "last_assistant_message": "done"}),
    )
    .unwrap();
    assert_eq!(stop.payload.state, State::Done);
    assert_eq!(stop.payload.prompt, "review");
}

#[test]
fn grok_aliases_strip_user_wrapper_and_drop_routine_approval_noise() {
    let mut state = HookListenerState::default();
    assert!(
        hook(
            &mut state,
            Source::Grok,
            json!({"hook_event_name": "SessionStart"})
        )
        .is_none()
    );
    let prompt = hook(&mut state, Source::Grok, json!({"hook_event_name": "user-prompt-submit", "prompt": "<user_query> review </user_query>"})).unwrap();
    assert_eq!(prompt.payload.prompt, "review");
    assert!(prompt.has_explicit_prompt);
    assert!(hook(&mut state, Source::Grok, json!({"hook_event_name": "Notification", "notificationType": "permission_prompt", "message": " Tool permission requested ", "level": "info"})).is_none());
    let question = hook(&mut state, Source::Grok, json!({"hook_event_name": "PreToolUse", "toolName": "ask_user_question", "toolInput": {"questions": [{"question": "Choose?"}]}})).unwrap();
    assert_eq!(question.payload.state, State::Waiting);
    assert!(question.payload.interactive_prompt.is_some());
    let answered = hook(&mut state, Source::Grok, json!({"hook_event_name": "PostToolUse", "toolName": "ask_user_question", "toolInput": {"questions": []}})).unwrap();
    assert_eq!(answered.payload.state, State::Working);
    assert!(answered.payload.interactive_prompt.is_none());
    let failed = hook(
        &mut state,
        Source::Grok,
        json!({"hook_event_name": "PostToolUseFailure", "error": "failed"}),
    )
    .unwrap();
    assert!(failed.payload.tool_name.is_none());
    assert_eq!(
        failed.payload.last_assistant_message.as_deref(),
        Some("failed")
    );
}

#[test]
fn antigravity_busy_stop_is_not_terminal_and_late_tools_cannot_resurrect_completion() {
    let mut state = HookListenerState::default();
    let started = hook(&mut state, Source::Antigravity, json!({"hook_event_name": "PreInvocation", "prompt": "review", "conversationId": "conversation-1"})).unwrap();
    assert_eq!(started.provider_session.unwrap().key, "conversation_id");
    let busy = hook(&mut state, Source::Antigravity, json!({"hook_event_name": "Stop", "fullyIdle": false, "transcriptPath": "transcript", "last_assistant_message": "premature"})).unwrap();
    assert_eq!(busy.payload.state, State::Working);
    assert!(busy.payload.last_assistant_message.is_none());
    let question = hook(&mut state, Source::Antigravity, json!({"hook_event_name": "PreToolUse", "toolCall": {"name": "ask_question", "args": {"question": "Choose?"}}, "transcriptPath": "transcript"})).unwrap();
    assert_eq!(question.payload.state, State::Waiting);
    assert_eq!(question.payload.tool_input.as_deref(), Some("Choose?"));
    let done = hook(&mut state, Source::Antigravity, json!({"hook_event_name": "Stop", "transcriptPath": "transcript", "last_assistant_message": "complete"})).unwrap();
    assert_eq!(done.payload.state, State::Done);
    assert!(
        hook(
            &mut state,
            Source::Antigravity,
            json!({"hook_event_name": "PostToolUse", "transcriptPath": "transcript"})
        )
        .is_none()
    );
    assert!(
        hook(
            &mut state,
            Source::Antigravity,
            json!({"hook_event_name": "PreInvocation", "transcriptPath": "transcript"})
        )
        .is_some()
    );
}

#[test]
fn antigravity_accepts_absent_empty_and_whitespace_stdin_with_envelope_event() {
    for raw in [None, Some(""), Some(" \r\n\t ")] {
        let result = normalize_hook_payload(
            &mut HookListenerState::default(),
            Source::Antigravity,
            json!({"paneKey": PANE, "hook_event_name": "PreInvocation"})
                .as_object()
                .unwrap(),
            raw,
            "production",
        )
        .unwrap();
        assert_eq!(result.payload.state, State::Working);
        assert_eq!(result.payload.prompt, "");
    }
}

#[test]
fn windows_cursor_stdin_strips_one_leading_bom() {
    let payload = format!(
        "\u{feff}{}",
        json!({"hook_event_name": "beforeSubmitPrompt", "prompt": "review"})
    );
    let event = normalize_hook_payload(
        &mut HookListenerState::default(),
        Source::Cursor,
        json!({"paneKey": PANE}).as_object().unwrap(),
        Some(&payload),
        "production",
    )
    .unwrap();
    assert_eq!(event.payload.state, State::Working);
    assert_eq!(event.payload.prompt, "review");
    let doubled = format!("\u{feff}{payload}");
    assert_eq!(
        normalize_hook_payload(
            &mut HookListenerState::default(),
            Source::Cursor,
            json!({"paneKey": PANE}).as_object().unwrap(),
            Some(&doubled),
            "production"
        ),
        Err(NormalizeError::NoPayload)
    );
}

#[test]
fn amp_threads_isolate_prompts_and_drop_delayed_tools_after_end() {
    let mut state = HookListenerState::default();
    for (thread, prompt) in [("one", "first"), ("two", "second")] {
        hook(
            &mut state,
            Source::Amp,
            json!({"hook_event_name": "agent.start", "threadId": thread, "prompt": prompt}),
        );
    }
    let tool = hook(&mut state, Source::Amp, json!({"hook_event_name": "tool.result", "threadId": "one", "tool": "plugin", "input": {"query": "needle"}, "message": "output text"})).unwrap();
    assert_eq!(tool.payload.prompt, "first");
    assert_eq!(tool.payload.tool_input.as_deref(), Some("needle"));
    assert_eq!(
        tool.payload.last_assistant_message.as_deref(),
        Some("output text")
    );
    assert!(!tool.has_explicit_prompt);
    let end = hook(
        &mut state,
        Source::Amp,
        json!({"hook_event_name": "agent.end", "threadId": "one", "status": "cancelled"}),
    )
    .unwrap();
    assert!(end.payload.interrupted);
    assert_eq!(end.payload.state, State::Done);
    assert!(
        hook(
            &mut state,
            Source::Amp,
            json!({"hook_event_name": "tool.call", "threadId": "one"})
        )
        .is_none()
    );
    let other = hook(
        &mut state,
        Source::Amp,
        json!({"hook_event_name": "tool.call", "threadId": "two"}),
    )
    .unwrap();
    assert_eq!(other.payload.prompt, "second");
}

#[test]
fn amp_evicts_oldest_scoped_threads_and_pane_teardown_clears_their_caches() {
    let mut state = HookListenerState::default();
    for index in 0..40 {
        hook(
            &mut state,
            Source::Amp,
            json!({"hook_event_name": "agent.start", "threadId": format!("thread-{index}"), "prompt": format!("prompt-{index}")}),
        );
    }
    assert_eq!(state.last_prompt_by_pane_key.len(), 32);
    assert_eq!(state.last_tool_by_pane_key.len(), 32);
    assert!(
        !state
            .last_prompt_by_pane_key
            .contains_key(&format!("{PANE}\0amp:thread-0"))
    );
    hook(
        &mut state,
        Source::Amp,
        json!({"hook_event_name": "agent.end", "threadId": "thread-39"}),
    );
    clear_pane_cache_state(&mut state, PANE);
    assert!(state.last_prompt_by_pane_key.is_empty());
    assert!(state.last_tool_by_pane_key.is_empty());
    assert!(state.amp_completed_cache_keys.is_empty());
}

#[test]
fn copilot_permission_gates_work_while_questions_and_notifications_block() {
    let mut state = HookListenerState::default();
    hook(
        &mut state,
        Source::Copilot,
        json!({"hook_event_name": "userPromptSubmitted", "prompt": "review"}),
    );
    let permission = hook(&mut state, Source::Copilot, json!({"hook_event_name": "permissionRequest", "toolName": "Bash", "toolInput": {"command": "git status"}})).unwrap();
    assert_eq!(permission.payload.state, State::Working);
    let question = hook(&mut state, Source::Copilot, json!({"hook_event_name": "preToolUse", "toolCalls": [{"name": "ask_user", "args": "{\"question\":\"Choose?\"}"}]})).unwrap();
    assert_eq!(question.payload.state, State::Blocked);
    assert_eq!(
        question.payload.last_assistant_message.as_deref(),
        Some("Choose?")
    );
    let notification = hook(&mut state, Source::Copilot, json!({"hook_event_name": "notification", "notificationType": "elicitation_dialog", "message": "Please approve"})).unwrap();
    assert_eq!(notification.payload.state, State::Blocked);
    assert_eq!(notification.payload.prompt, "review");
    assert_eq!(
        notification.payload.last_assistant_message.as_deref(),
        Some("Please approve")
    );
}

#[test]
fn copilot_infers_missing_event_types_and_clears_stale_stop_output() {
    let mut state = HookListenerState::default();
    let initial = hook(
        &mut state,
        Source::Copilot,
        json!({"initial_prompt": "review"}),
    )
    .unwrap();
    assert_eq!(initial.payload.state, State::Working);
    let result = hook(
        &mut state,
        Source::Copilot,
        json!({"toolName": "Bash", "tool_result": "old output"}),
    )
    .unwrap();
    assert_eq!(
        result.payload.last_assistant_message.as_deref(),
        Some("old output")
    );
    let stop = hook(&mut state, Source::Copilot, json!({"stop_reason": "done"})).unwrap();
    assert_eq!(stop.payload.state, State::Done);
    assert!(stop.payload.last_assistant_message.is_none());
    for (recoverable, expected) in [(true, State::Working), (false, State::Done)] {
        let error = hook(&mut state, Source::Copilot, json!({"hook_event_name": "errorOccurred", "error": "failed", "recoverable": recoverable})).unwrap();
        assert_eq!(error.payload.state, expected);
        assert!(error.payload.tool_name.is_none());
    }
}
