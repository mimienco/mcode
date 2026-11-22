//! Wire fixtures ported from Electron's server-{codex,gemini}-normalization tests.

use mcode_agent_status::listener_state::HookListenerState;
use mcode_agent_status::normalizers::{NormalizeError, normalize_hook_payload};
use mcode_agent_status::types::{
    AgentHookEventPayload, AgentHookSource, AgentStatusState, AgentSubagentState,
};
use serde_json::{Value, json};

const PANE: &str = "tab-1:3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a";

fn hook(
    state: &mut HookListenerState,
    source: AgentHookSource,
    payload: Value,
) -> AgentHookEventPayload {
    normalize_hook_payload(
        state,
        source,
        json!({
            "paneKey": PANE, "tabId": "tab-1", "worktreeId": "folder-workspace",
            "launchToken": "launch-1", "version": "1", "env": "production"
        })
        .as_object()
        .unwrap(),
        Some(&payload.to_string()),
        "production",
    )
    .unwrap()
}

#[test]
fn gemini_native_and_legacy_tools_use_ordered_input_aliases() {
    for event in ["BeforeTool", "AfterTool", "PreToolUse", "PostToolUse"] {
        for key in ["tool_input", "args", "input"] {
            let mut state = HookListenerState::default();
            let mut payload = json!({"hook_event_name": event, "tool_name": "read_file"});
            payload[key] = json!({"path": "/src/index.ts"});
            let result = hook(&mut state, AgentHookSource::Gemini, payload);
            assert_eq!(result.payload.state, AgentStatusState::Working);
            assert_eq!(result.payload.tool_name.as_deref(), Some("read_file"));
            assert_eq!(result.payload.tool_input.as_deref(), Some("/src/index.ts"));
        }
    }
}

#[test]
fn gemini_completion_retains_prompt_and_new_turn_resets_tool_and_answer() {
    let mut state = HookListenerState::default();
    let started = hook(
        &mut state,
        AgentHookSource::Gemini,
        json!({
            "hook_event_name": "BeforeAgent", "message": " review code ", "session_id": "gemini-session"
        }),
    );
    assert!(!started.has_explicit_prompt);
    assert_eq!(started.provider_session.unwrap().id, "gemini-session");
    hook(
        &mut state,
        AgentHookSource::Gemini,
        json!({
            "hook_event_name": "BeforeTool", "name": "run_shell_command", "args": {"command": "git status"}
        }),
    );
    let done = hook(
        &mut state,
        AgentHookSource::Gemini,
        json!({
            "hook_event_name": "AfterAgent", "prompt_response": "Done\r\nTests passed."
        }),
    );
    assert_eq!(done.payload.state, AgentStatusState::Done);
    assert_eq!(done.payload.prompt, "review code");
    assert_eq!(
        done.payload.last_assistant_message.as_deref(),
        Some("Done\nTests passed.")
    );
    assert_eq!(done.payload.tool_input.as_deref(), Some("git status"));
    let next = hook(
        &mut state,
        AgentHookSource::Gemini,
        json!({"hook_event_name": "BeforeAgent"}),
    );
    assert_eq!(next.payload.prompt, "");
    assert!(next.payload.tool_name.is_none());
    assert!(next.payload.tool_input.is_none());
    assert!(next.payload.last_assistant_message.is_none());
}

#[test]
fn codex_nested_children_preserve_lead_identity_and_own_waits() {
    let mut state = HookListenerState::default();
    let lead = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({
            "hook_event_name": "UserPromptSubmit", "prompt": "Coordinate the review", "model": "gpt-5.4",
            "session_id": "root-session", "transcript_path": "/root/transcript.jsonl"
        }),
    );
    assert!(lead.has_explicit_prompt);
    assert_eq!(lead.connection_id, None);
    assert_eq!(lead.worktree_id.as_deref(), Some("folder-workspace"));
    assert_eq!(lead.provider_session.unwrap().id, "root-session");
    let started = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({
            "hook_event_name": "SubagentStart", "agent_id": "child", "session_id": "child-session",
            "agent_type": "reviewer", "model": "gpt-5.4-mini", "prompt": "child-only prompt"
        }),
    );
    assert!(started.provider_session.is_none());
    assert_eq!(started.payload.prompt, "Coordinate the review");
    assert_eq!(started.payload.model.as_deref(), Some("gpt-5.4"));
    let child = &started.payload.subagents.unwrap()[0];
    assert_eq!(child.agent_type.as_deref(), Some("reviewer"));
    assert_eq!(child.model.as_deref(), Some("gpt-5.4-mini"));
    for (event, expected) in [
        ("PermissionRequest", AgentStatusState::Waiting),
        ("PreToolUse", AgentStatusState::Working),
    ] {
        let result = hook(
            &mut state,
            AgentHookSource::Codex,
            json!({
                "hook_event_name": event, "agent_id": "child", "tool_name": "exec_command", "tool_input": {"cmd": "git fetch"}
            }),
        );
        assert_eq!(result.payload.state, expected);
        assert_eq!(
            result.payload.subagents.unwrap()[0].started_at,
            child.started_at
        );
        assert!(result.payload.tool_name.is_none());
    }
    let done = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "Stop"}),
    );
    assert_eq!(done.payload.state, AgentStatusState::Done);
    assert!(done.payload.subagents.is_none());
    let resumed = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "PreToolUse", "agent_id": "child"}),
    );
    assert_eq!(resumed.payload.state, AgentStatusState::Working);
    let stopped = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "SubagentStop", "agent_id": "child"}),
    );
    assert_eq!(stopped.payload.state, AgentStatusState::Done);
    assert!(stopped.payload.subagents.is_none());
}

#[test]
fn codex_question_wait_preserves_raw_input_and_clears_after_answer() {
    let mut state = HookListenerState::default();
    let questions = json!({"questions": [{"id": "choice", "question": "Choose?", "options": [{"label": "Yes"}]}]});
    let waiting = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({
            "hook_event_name": "PreToolUse", "name": "request_user_input", "arguments": questions
        }),
    );
    assert_eq!(waiting.payload.state, AgentStatusState::Waiting);
    assert_eq!(
        serde_json::from_str::<Value>(&waiting.payload.interactive_prompt.unwrap()).unwrap(),
        questions
    );
    let answered = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({
            "hook_event_name": "PostToolUse", "name": "request_user_input", "arguments": questions
        }),
    );
    assert_eq!(answered.payload.state, AgentStatusState::Working);
    assert!(answered.payload.interactive_prompt.is_none());
}

#[test]
fn codex_tool_aliases_and_permission_approval_match_wire_contract() {
    let mut state = HookListenerState::default();
    for key in ["tool_input", "input", "arguments"] {
        let mut payload = json!({"hook_event_name": "PermissionRequest", "name": "exec_command"});
        payload[key] = json!({"cmd": "git fetch"});
        let event = hook(&mut state, AgentHookSource::Codex, payload);
        assert_eq!(event.payload.state, AgentStatusState::Waiting);
        assert_eq!(event.payload.tool_input.as_deref(), Some("git fetch"));
        let approval: Value =
            serde_json::from_str(&event.payload.interactive_prompt.unwrap()).unwrap();
        assert_eq!(approval["approval"]["tool"], "exec_command");
    }
}

#[test]
fn codex_new_session_clears_model_prompt_tools_answer_and_children() {
    let mut state = HookListenerState::default();
    hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "UserPromptSubmit", "prompt": "old", "model": "old-model"}),
    );
    hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "PreToolUse", "tool_name": "exec_command", "input": {"cmd": "git status"}}),
    );
    let done = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "Stop", "last_assistant_message": "All done"}),
    );
    assert_eq!(
        done.payload.last_assistant_message.as_deref(),
        Some("All done")
    );
    hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "SubagentStart", "agent_id": "child"}),
    );
    let next = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({"hook_event_name": "SessionStart"}),
    );
    assert_eq!(next.payload.state, AgentStatusState::Working);
    assert!(next.payload.prompt.is_empty());
    assert!(next.payload.model.is_none());
    assert!(next.payload.tool_name.is_none());
    assert!(next.payload.last_assistant_message.is_none());
    assert!(next.payload.subagents.is_none());
}

#[test]
fn codex_new_prompt_ignores_spurious_tool_fields() {
    let mut state = HookListenerState::default();
    let event = hook(
        &mut state,
        AgentHookSource::Codex,
        json!({
            "hook_event_name": "UserPromptSubmit", "prompt": "real ask", "tool_name": "exec_command", "input": {"cmd": "noise"}
        }),
    );
    assert!(event.payload.tool_name.is_none());
    assert!(event.payload.tool_input.is_none());
}

#[test]
fn codex_child_stop_hook_requires_id_and_unknown_events_are_dropped() {
    for (source, event) in [
        (AgentHookSource::Codex, "SubagentStop"),
        (AgentHookSource::Codex, "unknown"),
        (AgentHookSource::Gemini, "PermissionRequest"),
    ] {
        let result = normalize_hook_payload(
            &mut HookListenerState::default(),
            source,
            json!({"paneKey": PANE}).as_object().unwrap(),
            Some(&json!({"hook_event_name": event}).to_string()),
            "production",
        );
        assert_eq!(result, Err(NormalizeError::NoPayload));
    }
}

#[test]
fn codex_roster_is_bounded_and_child_wait_overrides_lead_done() {
    use mcode_agent_status::normalizers::codex_roster::{
        CodexSubagentRoster, effective_state, snapshots, upsert,
    };
    let mut roster = CodexSubagentRoster::new();
    upsert(&mut roster, " ", None, None, AgentSubagentState::Working, 1);
    upsert(
        &mut roster,
        &"x".repeat(65),
        None,
        None,
        AgentSubagentState::Working,
        1,
    );
    assert!(roster.is_empty());
    for index in 0..40 {
        upsert(
            &mut roster,
            &format!("child-{index:02}"),
            None,
            Some("model"),
            AgentSubagentState::Working,
            index,
        );
    }
    assert_eq!(roster.len(), 32);
    upsert(
        &mut roster,
        " child-00 ",
        Some("reviewer"),
        None,
        AgentSubagentState::Waiting,
        999,
    );
    assert_eq!(roster["child-00"].started_at, 0);
    assert_eq!(roster["child-00"].model.as_deref(), Some("model"));
    assert_eq!(
        effective_state(Some(&roster), AgentStatusState::Done),
        AgentStatusState::Waiting
    );
    assert_eq!(snapshots(Some(&roster)).unwrap()[0].id, "child-00");
}

#[test]
fn installed_hook_form_preserves_plus_percent_and_routes_both_providers() {
    use mcode_agent_status::http::{
        HookRequestParts, HookServerConfig, handle_hook_request_with_status,
    };
    for (route, event, agent) in [
        ("/hook/codex", "UserPromptSubmit", "codex"),
        ("/hook/gemini", "BeforeAgent", "gemini"),
    ] {
        let payload = json!({"hook_event_name": event, "prompt": "check C++ at 50% & rerun", "session_id": "session-1"}).to_string();
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("paneKey", PANE)
            .append_pair("tabId", "tab-1")
            .append_pair("launchToken", "launch-1")
            .append_pair("payload", &payload)
            .finish();
        let (status, result) = handle_hook_request_with_status(
            &mut HookListenerState::default(),
            &mut Default::default(),
            &HookServerConfig {
                token: "token".to_string(),
                env: "production".to_string(),
            },
            HookRequestParts {
                method: "POST".to_string(),
                path: route.to_string(),
                token_header: Some("token".to_string()),
                content_type: Some("application/x-www-form-urlencoded".to_string()),
                body: body.into_bytes(),
            },
            1000,
        );
        assert_eq!(status, 204);
        let result = result.unwrap();
        assert_eq!(result.payload.agent_type.as_deref(), Some(agent));
        assert_eq!(result.payload.prompt, "check C++ at 50% & rerun");
        assert_eq!(result.launch_token.as_deref(), Some("launch-1"));
    }
}

#[test]
fn cursor_execution_gates_work_and_failures_clear_tools_but_surface_errors() {
    let mut state = HookListenerState::default();
    for (payload, name, input) in [
        (
            json!({"hook_event_name": "beforeShellExecution", "command": "git status"}),
            "Shell",
            "git status",
        ),
        (
            json!({"hook_event_name": "beforeMCPExecution", "url": "https://example.com"}),
            "MCP",
            "https://example.com",
        ),
    ] {
        let result = hook(&mut state, AgentHookSource::Cursor, payload);
        assert_eq!(result.payload.state, AgentStatusState::Working);
        assert_eq!(result.payload.tool_name.as_deref(), Some(name));
        assert_eq!(result.payload.tool_input.as_deref(), Some(input));
    }
    let failed = hook(
        &mut state,
        AgentHookSource::Cursor,
        json!({
            "hook_event_name": "postToolUseFailure", "tool_output": {"content": [{"text": "Permission denied"}]}, "error_message": "fallback"
        }),
    );
    assert_eq!(failed.payload.state, AgentStatusState::Working);
    assert!(failed.payload.tool_name.is_none());
    assert!(failed.payload.tool_input.is_none());
    assert_eq!(
        failed.payload.last_assistant_message.as_deref(),
        Some("Permission denied")
    );
}

#[test]
fn cursor_late_final_response_enriches_done_without_resurrecting_work() {
    let mut state = HookListenerState::default();
    hook(
        &mut state,
        AgentHookSource::Cursor,
        json!({"hook_event_name": "beforeSubmitPrompt", "prompt": "review"}),
    );
    let done = hook(
        &mut state,
        AgentHookSource::Cursor,
        json!({"hook_event_name": "stop", "status": "completed"}),
    );
    assert!(!done.payload.interrupted);
    state.last_status_by_pane_key.insert(PANE.to_string(), done);
    let response = hook(
        &mut state,
        AgentHookSource::Cursor,
        json!({"hook_event_name": "afterAgentResponse", "text": "Review complete"}),
    );
    assert_eq!(response.payload.state, AgentStatusState::Done);
    assert_eq!(
        response.payload.last_assistant_message.as_deref(),
        Some("Review complete")
    );
    let next = hook(
        &mut state,
        AgentHookSource::Cursor,
        json!({"hook_event_name": "sessionStart"}),
    );
    assert_eq!(next.payload.state, AgentStatusState::Working);
    assert!(next.payload.prompt.is_empty());
    assert!(next.payload.last_assistant_message.is_none());
}

#[test]
fn cursor_stop_only_marks_explicit_noncompleted_status_as_interrupted() {
    for (status, interrupted) in [
        (Value::Null, false),
        (json!(42), false),
        (json!("completed"), false),
        (json!("aborted"), true),
    ] {
        let event = hook(
            &mut HookListenerState::default(),
            AgentHookSource::Cursor,
            json!({"hook_event_name": "stop", "status": status}),
        );
        assert_eq!(event.payload.interrupted, interrupted);
    }
}

#[test]
fn pi_family_session_start_captures_authoritative_session_file() {
    for source in [
        AgentHookSource::Pi,
        AgentHookSource::Omp,
        AgentHookSource::PrimeAgent,
    ] {
        let event = hook(
            &mut HookListenerState::default(),
            source,
            json!({
                "hook_event_name": "session_start", "session_id": "session-1", "session_file": " /sessions/active.jsonl "
            }),
        );
        assert!(event.provider_session_only);
        assert_eq!(event.payload.state, AgentStatusState::Done);
        assert_eq!(
            event.provider_session.unwrap().transcript_path.as_deref(),
            Some("/sessions/active.jsonl")
        );
    }
}

#[test]
fn transcript_metadata_rejects_control_characters_and_codex_empty_child_id_is_lead() {
    let event = hook(
        &mut HookListenerState::default(),
        AgentHookSource::Codex,
        json!({
            "hook_event_name": "SessionStart", "agent_id": "", "session_id": "lead", "transcript_path": "bad\npath", "transcriptPath": " good-path "
        }),
    );
    let metadata = event.provider_session.unwrap();
    assert_eq!(metadata.id, "lead");
    assert_eq!(metadata.transcript_path.as_deref(), Some("good-path"));
}
