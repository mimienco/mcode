//! Inference endpoints — Rust port of `inferInterrupt`
//! (`server.ts:906-1002`) + `inferQuestionAnswered` (`:1004-1060`). Strict
//! baseline matching keeps a delayed inference from clobbering a newer hook.

use crate::listener_state::HookListenerState;
use crate::normalizers::ask_user::{is_agent_interrupt_input_intent, is_ask_user_question_tool};
use crate::normalizers::claude::{
    clear_claude_answered_question_wait, mark_claude_lead_turn_interrupted,
};
use crate::types::{
    AgentInterruptInferenceRequest, AgentQuestionAnsweredInferenceRequest, AgentStatusIpcPayload,
    AgentStatusState, EnrichedAgentHookEventPayload,
};

/// The oracle only aliases the unknown sentinel to absent identity.
fn interrupt_agent_types_equivalent(cached: Option<&str>, baseline: Option<&str>) -> bool {
    cached.filter(|value| *value != "unknown") == baseline.filter(|value| *value != "unknown")
}

/// `inferInterrupt` (`server.ts:906-1002`).
pub fn infer_interrupt(
    state: &mut HookListenerState,
    enriched: &mut std::collections::HashMap<String, EnrichedAgentHookEventPayload>,
    request: &AgentInterruptInferenceRequest,
    now: i64,
) -> bool {
    if !crate::types::is_valid_pane_key(&request.pane_key) {
        return false;
    }
    if !is_agent_interrupt_input_intent(&request.intent) {
        return false;
    }
    let Some(existing) = enriched.get(&request.pane_key).cloned() else {
        return false;
    };
    if existing.event.provider_session_only {
        return false;
    }
    // Why: inference must not fabricate a `done` onto a row whose `working`
    // was never confirmed this runtime.
    if existing.restored_unconfirmed || existing.event.provider_session_only {
        return false;
    }
    let payload = &existing.event.payload;
    let agent_type = payload.agent_type.clone();
    // Why: Droid's Ctrl+C exits the CLI (handled by PTY lifecycle) rather than
    // interrupting the current turn.
    if agent_type.as_deref() == Some("droid") && request.intent == "ctrl-c" {
        return false;
    }
    // Why: these agents use the first Escape as a TUI cancel that can leave
    // the turn running; only a double Escape infers an interrupt.
    if matches!(agent_type.as_deref(), Some("opencode") | Some("copilot"))
        && request.intent == "plain-escape"
        && request.input_count != Some(2)
    {
        return false;
    }
    let dismisses_claude_question = agent_type.as_deref() == Some("claude")
        && request.intent == "plain-escape"
        && payload.state == AgentStatusState::Waiting
        && is_ask_user_question_tool(payload.tool_name.as_deref());
    if dismisses_claude_question {
        return infer_question_answered(
            state,
            enriched,
            &AgentQuestionAnsweredInferenceRequest {
                pane_key: request.pane_key.clone(),
                baseline_updated_at: request.baseline_updated_at,
                baseline_state_started_at: request.baseline_state_started_at,
                baseline_prompt: request.baseline_prompt.clone(),
                baseline_agent_type: request.baseline_agent_type.clone(),
            },
            now,
        );
    }
    // Why: inference is a fallback for a missing final hook; a strict baseline
    // match keeps a delayed timer from clobbering any newer hook.
    if payload.state != AgentStatusState::Working
        || !interrupt_agent_types_equivalent(
            agent_type.as_deref(),
            request.baseline_agent_type.as_deref(),
        )
        || payload.prompt != request.baseline_prompt
        || existing.received_at != request.baseline_updated_at
        || existing.state_started_at != request.baseline_state_started_at
        || now.saturating_sub(existing.received_at) > crate::types::AGENT_STATUS_STALE_AFTER_MS
    {
        return false;
    }
    // Why: a 'working' pane can be child-driven; Ctrl+C doesn't stop
    // background children, so inferring done would retire live child rows.
    if payload.subagents.as_ref().is_some_and(|subs| {
        subs.iter()
            .any(|sub| sub.state != crate::types::AgentSubagentState::Idle)
    }) {
        return false;
    }
    // Why: Escape/Ctrl+C at Claude's idle prompt does not stop provider-owned
    // shells or session crons.
    if agent_type.as_deref() == Some("claude")
        && (state
            .claude_running_non_agent_task_pane_keys
            .contains(&existing.event.pane_key)
            || state
                .claude_active_session_cron_pane_keys
                .contains(&existing.event.pane_key))
    {
        return false;
    }
    // Why: keep the Claude lead-turn record in sync, or a later child event
    // re-emits the stale 'working' state and resurrects the cancelled pane.
    if agent_type.as_deref() == Some("claude") {
        mark_claude_lead_turn_interrupted(state, &existing.event.pane_key);
    }
    if agent_type.as_deref() == Some("codex") {
        state.codex_lead_state_by_pane_key.insert(
            existing.event.pane_key.clone(),
            crate::listener_state::CodexLeadTurnState {
                state: AgentStatusState::Done,
                model: payload.model.clone(),
            },
        );
    }
    let inferred_payload = crate::types::AgentHookEventPayload {
        pane_key: existing.event.pane_key.clone(),
        source: existing.event.source,
        launch_token: existing.event.launch_token.clone(),
        tab_id: existing.event.tab_id.clone(),
        worktree_id: existing.event.worktree_id.clone(),
        connection_id: existing.event.connection_id.clone(),
        has_explicit_prompt: existing.event.has_explicit_prompt,
        restored_unconfirmed: existing.event.restored_unconfirmed,
        prompt_interaction_key: existing.event.prompt_interaction_key.clone(),
        hook_event_name: existing.event.hook_event_name.clone(),
        provider_prompt_id: existing.event.provider_prompt_id.clone(),
        compact_trigger: None,
        tool_use_id: existing.event.tool_use_id.clone(),
        tool_agent_id: existing.event.tool_agent_id.clone(),
        teammate_name: existing.event.teammate_name.clone(),
        tool_agent_type: existing.event.tool_agent_type.clone(),
        provider_session: existing.event.provider_session.clone(),
        provider_session_only: existing.event.provider_session_only,
        is_replay: existing.event.is_replay,
        claude_running_non_agent_task: existing.event.claude_running_non_agent_task,
        payload: crate::types::ParsedAgentStatusPayload {
            state: AgentStatusState::Done,
            prompt: payload.prompt.clone(),
            agent_type: agent_type.clone(),
            model: payload.model.clone(),
            interrupted: true,
            // Why: idle children are display state; dropping them on an
            // inferred interrupt blanks rows a later hook would restore.
            subagents: payload.subagents.clone(),
            ..crate::types::ParsedAgentStatusPayload::default()
        },
    };
    crate::store::apply_enriched(state, enriched, inferred_payload, now).is_some()
}

/// `inferQuestionAnswered` (`server.ts:1004-1060`).
pub fn infer_question_answered(
    state: &mut HookListenerState,
    enriched: &mut std::collections::HashMap<String, EnrichedAgentHookEventPayload>,
    request: &AgentQuestionAnsweredInferenceRequest,
    now: i64,
) -> bool {
    if !crate::types::is_valid_pane_key(&request.pane_key) {
        return false;
    }
    let Some(existing) = enriched.get(&request.pane_key).cloned() else {
        return false;
    };
    // Why: inference must not fabricate a transition onto a row whose state
    // was never confirmed this runtime.
    if existing.restored_unconfirmed || existing.event.provider_session_only {
        return false;
    }
    let payload = &existing.event.payload;
    // Why: only Claude's interactive question clears on typed input — tool
    // name (not hook event) discriminates; real permission waits stay sticky.
    if payload.agent_type.as_deref() != Some("claude")
        || payload.state != AgentStatusState::Waiting
        || !is_ask_user_question_tool(payload.tool_name.as_deref())
    {
        return false;
    }
    if payload.agent_type != request.baseline_agent_type
        || payload.prompt != request.baseline_prompt
        || existing.received_at != request.baseline_updated_at
        || existing.state_started_at != request.baseline_state_started_at
        || now.saturating_sub(existing.received_at) > crate::types::AGENT_STATUS_STALE_AFTER_MS
    {
        return false;
    }
    // Why: sync the listener's lead-turn record too, or a later child event
    // re-emits the stale waiting state and resurrects the card.
    let (restored_state, restored_interrupted, restored_turn_completed_at) =
        clear_claude_answered_question_wait(state, &existing.event.pane_key);
    let inferred_payload = crate::types::AgentHookEventPayload {
        pane_key: existing.event.pane_key.clone(),
        source: existing.event.source,
        launch_token: existing.event.launch_token.clone(),
        tab_id: existing.event.tab_id.clone(),
        worktree_id: existing.event.worktree_id.clone(),
        connection_id: existing.event.connection_id.clone(),
        has_explicit_prompt: existing.event.has_explicit_prompt,
        restored_unconfirmed: existing.event.restored_unconfirmed,
        prompt_interaction_key: existing.event.prompt_interaction_key.clone(),
        hook_event_name: existing.event.hook_event_name.clone(),
        provider_prompt_id: existing.event.provider_prompt_id.clone(),
        compact_trigger: None,
        tool_use_id: existing.event.tool_use_id.clone(),
        tool_agent_id: existing.event.tool_agent_id.clone(),
        teammate_name: existing.event.teammate_name.clone(),
        tool_agent_type: existing.event.tool_agent_type.clone(),
        provider_session: existing.event.provider_session.clone(),
        provider_session_only: existing.event.provider_session_only,
        is_replay: existing.event.is_replay,
        claude_running_non_agent_task: existing.event.claude_running_non_agent_task,
        payload: crate::types::ParsedAgentStatusPayload {
            state: restored_state,
            prompt: payload.prompt.clone(),
            agent_type: payload.agent_type.clone(),
            interrupted: restored_state == AgentStatusState::Done && restored_interrupted,
            turn_completed_at: restored_turn_completed_at,
            subagents: payload.subagents.clone(),
            ..crate::types::ParsedAgentStatusPayload::default()
        },
    };
    crate::store::apply_enriched(state, enriched, inferred_payload, now).is_some()
}

#[allow(dead_code)]
fn unused(_: &AgentStatusIpcPayload) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ParsedAgentStatusPayload;

    fn working_event(pane: &str, prompt: &str) -> crate::types::AgentHookEventPayload {
        crate::types::AgentHookEventPayload {
            pane_key: pane.to_string(),
            source: Some(crate::types::AgentHookSource::Claude),
            connection_id: None,
            payload: ParsedAgentStatusPayload {
                state: AgentStatusState::Working,
                prompt: prompt.to_string(),
                agent_type: Some("claude".into()),
                ..ParsedAgentStatusPayload::default()
            },
            ..crate::types::AgentHookEventPayload::default()
        }
    }

    const UUID: &str = "3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a";

    #[test]
    fn interrupt_identity_matches_exact_provider_with_unknown_as_absent() {
        assert!(!interrupt_agent_types_equivalent(
            Some("claude"),
            Some("codex")
        ));
        assert!(!interrupt_agent_types_equivalent(
            Some("devin"),
            Some("copilot")
        ));
        assert!(interrupt_agent_types_equivalent(Some("unknown"), None));
        assert!(interrupt_agent_types_equivalent(None, Some("unknown")));
        assert!(interrupt_agent_types_equivalent(
            Some("claude"),
            Some("claude")
        ));
    }

    #[test]
    fn interrupt_infers_done_on_strict_baseline_match() {
        let mut state = HookListenerState::default();
        let mut enriched = std::collections::HashMap::new();
        let applied = crate::store::apply_enriched(
            &mut state,
            &mut enriched,
            working_event(&format!("t:{UUID}"), "fix the bug"),
            1_000,
        );
        assert!(applied.is_some());
        let request = AgentInterruptInferenceRequest {
            pane_key: format!("t:{UUID}"),
            baseline_updated_at: applied.as_ref().unwrap().received_at,
            baseline_state_started_at: applied.as_ref().unwrap().state_started_at,
            baseline_prompt: "fix the bug".into(),
            baseline_agent_type: Some("claude".into()),
            intent: "ctrl-c".into(),
            input_count: None,
        };
        assert!(infer_interrupt(&mut state, &mut enriched, &request, 1_100));
        assert_eq!(
            enriched
                .get(&format!("t:{UUID}"))
                .unwrap()
                .event
                .payload
                .state,
            AgentStatusState::Done
        );
        assert!(
            enriched
                .get(&format!("t:{UUID}"))
                .unwrap()
                .event
                .payload
                .interrupted
        );
    }

    #[test]
    fn interrupt_rejects_stale_or_mismatched_baseline() {
        let mut state = HookListenerState::default();
        let mut enriched = std::collections::HashMap::new();
        let applied = crate::store::apply_enriched(
            &mut state,
            &mut enriched,
            working_event(&format!("t:{UUID}"), "fix the bug"),
            1_000,
        );
        let base = applied.as_ref().unwrap();
        let mut request = AgentInterruptInferenceRequest {
            pane_key: format!("t:{UUID}"),
            baseline_updated_at: base.received_at + 1,
            baseline_state_started_at: base.state_started_at,
            baseline_prompt: "fix the bug".into(),
            baseline_agent_type: Some("claude".into()),
            intent: "ctrl-c".into(),
            input_count: None,
        };
        assert!(!infer_interrupt(&mut state, &mut enriched, &request, 1_100));
        request.baseline_updated_at = base.received_at;
        request.baseline_prompt = "different".into();
        assert!(!infer_interrupt(&mut state, &mut enriched, &request, 1_100));
        // Staleness: 31 minutes later.
        assert!(!infer_interrupt(
            &mut state,
            &mut enriched,
            &request,
            base.received_at + 31 * 60 * 1000
        ));
    }

    #[test]
    fn interrupt_rejects_restored_unconfirmed_rows() {
        let mut state = HookListenerState::default();
        let mut enriched = std::collections::HashMap::new();
        let mut event = working_event(&format!("t:{UUID}"), "p");
        event.payload.state = AgentStatusState::Working;
        let applied =
            crate::store::apply_enriched(&mut state, &mut enriched, event, 1_000).unwrap();
        let entry = enriched.get_mut(&format!("t:{UUID}")).unwrap();
        entry.restored_unconfirmed = true;
        let request = AgentInterruptInferenceRequest {
            pane_key: format!("t:{UUID}"),
            baseline_updated_at: applied.received_at,
            baseline_state_started_at: applied.state_started_at,
            baseline_prompt: "p".into(),
            baseline_agent_type: Some("claude".into()),
            intent: "ctrl-c".into(),
            input_count: None,
        };
        assert!(!infer_interrupt(&mut state, &mut enriched, &request, 1_100));
    }

    #[test]
    fn invalid_pane_key_rejected() {
        let mut state = HookListenerState::default();
        let mut enriched = std::collections::HashMap::new();
        let request = AgentInterruptInferenceRequest {
            pane_key: "bogus".into(),
            baseline_updated_at: 1,
            baseline_state_started_at: 1,
            baseline_prompt: String::new(),
            baseline_agent_type: None,
            intent: "ctrl-c".into(),
            input_count: None,
        };
        assert!(!infer_interrupt(&mut state, &mut enriched, &request, 1));
    }
}
