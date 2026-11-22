//! Claude normalizer — Rust port of `normalizeClaudeEvent` + lifecycle helpers
//! (`agent-hook-listener.ts:2619-3213`). Every guard comment cites a shipped
//! bugfix in the original; keep the ordering exact.

use serde_json::Value;

use crate::listener_state::{
    ClaudeLeadTurnState, HookListenerState, StashedLeadState, resolve_prompt,
};
use crate::normalizers::ask_user::is_ask_user_question_tool;
use crate::normalizers::claude_roster::{
    self, TrackedState, fold_claude_background_tasks_into_roster, get_or_create_roster,
    idle_claude_teammate_by_name, read_claude_background_agent_tasks, stop_claude_subagent,
    upsert_working_claude_subagent,
};
use crate::normalizers::fields::normalize_agent_status_object;
use crate::normalizers::harness_text::is_compact_continuation_user_turn_text;
use crate::normalizers::tool_snapshot::{
    ToolSnapshot, clear_active_tool_fields_update, derive_interactive_prompt,
    derive_tool_input_preview, extract_tool_response_text, has_own_field, read_string,
    resolve_tool_state, tool_update,
};
use crate::types::AgentHookSource;
use crate::types::{AgentStatusState, ParsedAgentStatusPayload};

/// `markClaudeLeadTurnInterrupted` (`agent-hook-listener.ts:2753-2757`).
pub fn mark_claude_lead_turn_interrupted(state: &mut HookListenerState, pane_key: &str) {
    state.claude_lead_state_by_pane_key.insert(
        pane_key.to_string(),
        ClaudeLeadTurnState {
            state: AgentStatusState::Done,
            interrupted: true,
            ..ClaudeLeadTurnState::default()
        },
    );
    state
        .claude_running_non_agent_task_pane_keys
        .remove(pane_key);
    state.claude_active_session_cron_pane_keys.remove(pane_key);
}

/// `clearClaudeAnsweredQuestionWait` (`agent-hook-listener.ts:2852-2879`).
pub fn clear_claude_answered_question_wait(
    state: &mut HookListenerState,
    pane_key: &str,
) -> (AgentStatusState, bool, Option<i64>) {
    let restored: StashedLeadState = match state.claude_lead_state_by_pane_key.get(pane_key) {
        Some(lead) if lead.state == AgentStatusState::Waiting => {
            lead.state_before_wait.clone().unwrap_or(StashedLeadState {
                state: AgentStatusState::Working,
                ..Default::default()
            })
        }
        _ => StashedLeadState {
            state: AgentStatusState::Working,
            ..Default::default()
        },
    };
    state.claude_lead_state_by_pane_key.insert(
        pane_key.to_string(),
        ClaudeLeadTurnState {
            state: restored.state,
            interrupted: restored.interrupted,
            turn_completed_at: restored.turn_completed_at,
            ..ClaudeLeadTurnState::default()
        },
    );
    let previous_tool = state.last_tool_by_pane_key.get(pane_key).cloned();
    state.last_tool_by_pane_key.insert(
        pane_key.to_string(),
        ToolSnapshot {
            last_assistant_message: previous_tool.and_then(|tool| tool.last_assistant_message),
            ..ToolSnapshot::default()
        },
    );
    let effective =
        resolve_claude_pane_state(state, pane_key, restored.state, restored.interrupted);
    if effective == restored.state {
        (
            restored.state,
            restored.interrupted,
            restored.turn_completed_at,
        )
    } else {
        (effective, restored.interrupted, restored.turn_completed_at)
    }
}

/// `updateClaudeRunningNonAgentTask` (`agent-hook-listener.ts:2631-2642`).
fn update_claude_running_non_agent_task(
    state: &mut HookListenerState,
    pane_key: &str,
    has_running_non_agent_task: bool,
    interrupted: bool,
) {
    if has_running_non_agent_task && !interrupted {
        state
            .claude_running_non_agent_task_pane_keys
            .insert(pane_key.to_string());
    } else {
        state
            .claude_running_non_agent_task_pane_keys
            .remove(pane_key);
    }
}

/// `resolveClaudePaneState` (`agent-hook-listener.ts:2644-2659`).
fn resolve_claude_pane_state(
    state: &HookListenerState,
    pane_key: &str,
    lead_state: AgentStatusState,
    lead_interrupted: bool,
) -> AgentStatusState {
    if lead_state != AgentStatusState::Done {
        return lead_state;
    }
    let roster = state.claude_subagent_rosters.get(pane_key);
    if claude_roster::claude_roster_has_working_subagent(roster)
        || (!lead_interrupted
            && (state
                .claude_running_non_agent_task_pane_keys
                .contains(pane_key)
                || state
                    .claude_active_session_cron_pane_keys
                    .contains(pane_key)))
    {
        AgentStatusState::Working
    } else {
        AgentStatusState::Done
    }
}

/// `clearClaudePendingWaitForAgent` (`agent-hook-listener.ts:2832-2849`).
fn clear_claude_pending_wait_for_agent(
    state: &mut HookListenerState,
    pane_key: &str,
    owns_wait: impl Fn(&str) -> bool,
) {
    let Some(lead) = state.claude_lead_state_by_pane_key.get(pane_key) else {
        return;
    };
    if lead.state != AgentStatusState::Waiting {
        return;
    }
    let Some(waiting_agent_id) = lead.waiting_agent_id.clone() else {
        return;
    };
    if !owns_wait(&waiting_agent_id) {
        return;
    }
    let restored = lead.state_before_wait.clone().unwrap_or(StashedLeadState {
        state: AgentStatusState::Working,
        ..Default::default()
    });
    state.claude_lead_state_by_pane_key.insert(
        pane_key.to_string(),
        ClaudeLeadTurnState {
            state: restored.state,
            interrupted: restored.interrupted,
            turn_completed_at: restored.turn_completed_at,
            ..ClaudeLeadTurnState::default()
        },
    );
    let previous_tool = state.last_tool_by_pane_key.get(pane_key).cloned();
    state.last_tool_by_pane_key.insert(
        pane_key.to_string(),
        ToolSnapshot {
            last_assistant_message: previous_tool.and_then(|tool| tool.last_assistant_message),
            ..ToolSnapshot::default()
        },
    );
}

/// `buildClaudeStatusPayload` (`agent-hook-listener.ts:3173-3212`).
#[allow(clippy::too_many_arguments)] // mirrors the Electron helper's argument list
fn build_claude_status_payload(
    state: &mut HookListenerState,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
    state_name: AgentStatusState,
    update_tool_snapshot: bool,
    interrupted: Option<bool>,
    session_boundary: Option<bool>,
    turn_completed_at: Option<i64>,
) -> Option<ParsedAgentStatusPayload> {
    let new_turn = crate::normalizers::is_new_turn_event(AgentHookSource::Claude, event_name);
    // Why: child-driven refreshes are roster bookkeeping, not lead tool
    // activity; read the cached snapshot without merging.
    let snapshot = if update_tool_snapshot {
        resolve_tool_state(
            &mut state.last_tool_by_pane_key,
            pane_key,
            extract_claude_tool_fields(event_name, hook_payload),
            new_turn,
        )
    } else {
        state
            .last_tool_by_pane_key
            .get(pane_key)
            .cloned()
            .unwrap_or_default()
    };
    let prompt = if update_tool_snapshot && new_turn {
        resolve_prompt(state, pane_key, prompt_text, true)
    } else {
        resolve_prompt(state, pane_key, prompt_text, false)
    };
    let subagents =
        claude_roster::claude_roster_to_snapshots(state.claude_subagent_rosters.get(pane_key));
    normalize_agent_status_object(
        &serde_json::json!({
            "state": state_name_str(state_name),
            "prompt": prompt,
            "agentType": "claude",
            "toolName": snapshot.tool_name,
            "toolInput": snapshot.tool_input,
            "interactivePrompt": snapshot.interactive_prompt,
            "lastAssistantMessage": snapshot.last_assistant_message,
            "interrupted": interrupted.unwrap_or(false),
            "sessionBoundary": session_boundary.unwrap_or(false),
            "turnCompletedAt": turn_completed_at,
            "subagents": subagents,
        })
        .as_object()
        .unwrap()
        .clone(),
    )
}

fn state_name_str(state: AgentStatusState) -> &'static str {
    match state {
        AgentStatusState::Working => "working",
        AgentStatusState::Blocked => "blocked",
        AgentStatusState::Waiting => "waiting",
        AgentStatusState::Done => "done",
    }
}

/// `extractClaudeToolFields` (`agent-hook-listener.ts:1595-1647`).
fn extract_claude_tool_fields(
    event_name: Option<&str>,
    hook_payload: &serde_json::Map<String, Value>,
) -> ToolSnapshot {
    let mut update = ToolSnapshot::default();
    if event_name == Some("PostToolUseFailure") {
        update = clear_active_tool_fields_update();
    } else if matches!(
        event_name,
        Some("PreToolUse") | Some("PostToolUse") | Some("PermissionRequest")
    ) {
        let tool_name = read_string(hook_payload, "tool_name");
        update = tool_update(
            tool_name.clone(),
            derive_tool_input_preview(tool_name.as_deref(), hook_payload.get("tool_input")),
            derive_interactive_prompt(
                tool_name.as_deref(),
                hook_payload.get("tool_input"),
                event_name,
            ),
            has_own_field(hook_payload, "tool_input"),
        );
    }
    if event_name == Some("PostToolUse")
        && let Some(response_text) = extract_tool_response_text(hook_payload.get("tool_response"))
    {
        update.last_assistant_message = Some(response_text);
    }
    if event_name == Some("PostToolUseFailure") {
        let error_text = extract_tool_response_text(hook_payload.get("tool_response"))
            .or_else(|| read_string(hook_payload, "error"))
            .or_else(|| read_string(hook_payload, "message"));
        if let Some(error_text) = error_text {
            update.last_assistant_message = Some(error_text);
        }
    }
    // Why: Stop's transcript scan is a wave-38 file-access slice; wave 37
    // accepts only the direct field (documented deviation, parity-tracked).
    if event_name == Some("Stop")
        && let Some(direct) = read_string(hook_payload, "last_assistant_message")
    {
        update.last_assistant_message = Some(direct);
    }
    update
}

/// `normalizeClaudeSubagentLifecycleEvent` (`agent-hook-listener.ts:2662-2750`).
fn normalize_claude_subagent_lifecycle_event(
    state: &mut HookListenerState,
    event_name: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
    now: i64,
) -> Option<ParsedAgentStatusPayload> {
    let lifecycle_field = if event_name == "TeammateIdle" {
        "teammate_name"
    } else {
        "agent_id"
    };
    let lifecycle_id = read_string(hook_payload, lifecycle_field)?;
    let cached_lead = state.claude_lead_state_by_pane_key.get(pane_key).cloned();
    let owns_unbacked_wait = cached_lead.as_ref().is_some_and(|lead| {
        lead.state == AgentStatusState::Waiting
            && lead.state_before_wait.is_none()
            && lead.waiting_agent_id.is_some()
            && (if event_name == "TeammateIdle" {
                claude_roster::claude_teammate_id_matches_name(
                    lead.waiting_agent_id.as_deref().unwrap_or_default(),
                    &lifecycle_id,
                )
            } else {
                lead.waiting_agent_id.as_deref() == Some(lifecycle_id.as_str())
            })
    });
    let has_cached_lead_evidence = cached_lead.is_some() && !owns_unbacked_wait;
    let mut ended_child_work = false;
    let mut ended_runtime_child_work = false;

    if event_name == "TeammateIdle" {
        if let Some(roster) = state.claude_subagent_rosters.get_mut(pane_key) {
            let was_working = roster.iter().any(|(id, tracked)| {
                tracked.state == TrackedState::Working
                    && claude_roster::claude_teammate_id_matches_name(id, &lifecycle_id)
            });
            ended_runtime_child_work |= was_working;
            idle_claude_teammate_by_name(roster, &lifecycle_id);
            ended_child_work = was_working;
        }
        clear_claude_pending_wait_for_agent(state, pane_key, |waiting_agent_id| {
            claude_roster::claude_teammate_id_matches_name(waiting_agent_id, &lifecycle_id)
        });
    } else if event_name == "SubagentStart" {
        let roster = get_or_create_roster(&mut state.claude_subagent_rosters, pane_key);
        upsert_working_claude_subagent(
            roster,
            &lifecycle_id,
            (&read_string(hook_payload, "agent_type"), &None),
            now,
        );
    } else {
        if let Some(roster) = state.claude_subagent_rosters.get_mut(pane_key) {
            let was_working = roster
                .get(&lifecycle_id)
                .is_some_and(|tracked| tracked.state == TrackedState::Working);
            ended_runtime_child_work = was_working
                && !roster
                    .get(&lifecycle_id)
                    .is_some_and(|tracked| tracked.restored_from_snapshot);
            stop_claude_subagent(roster, &lifecycle_id);
            ended_child_work = was_working
                && !roster
                    .get(&lifecycle_id)
                    .is_some_and(|tracked| tracked.state == TrackedState::Working);
        }
        // Why: a blocked child that dies without another tool event would pin
        // its permission/question wait on the pane forever.
        clear_claude_pending_wait_for_agent(state, pane_key, |waiting_agent_id| {
            waiting_agent_id == lifecycle_id
        });
    }
    let roster = state.claude_subagent_rosters.get(pane_key);
    let working_child_evidence = claude_roster::claude_roster_has_runtime_working_subagent(roster);
    let has_unconfirmed_child = claude_roster::claude_roster_has_restored_snapshot_subagent(roster);
    let has_confirmed_done_gate = cached_lead.as_ref().is_some_and(|lead| {
        lead.state == AgentStatusState::Done
            && !lead.interrupted
            && (state
                .claude_running_non_agent_task_pane_keys
                .contains(pane_key)
                || state
                    .claude_active_session_cron_pane_keys
                    .contains(pane_key))
    });
    let restored_only_done_gate = cached_lead
        .as_ref()
        .is_some_and(|lead| lead.state == AgentStatusState::Done)
        && !has_confirmed_done_gate
        && has_unconfirmed_child;
    if roster.is_some_and(|r| r.is_empty()) {
        state.claude_subagent_rosters.remove(pane_key);
    }
    if !working_child_evidence
        && (restored_only_done_gate
            || (ended_child_work
                && !has_cached_lead_evidence
                && (has_unconfirmed_child || !ended_runtime_child_work)))
    {
        // Why: a restored-only ending proves no lead boundary; persist the
        // roster transition without publishing fresh work or completion.
        state
            .claude_unconfirmed_restored_status_pane_keys
            .insert(pane_key.to_string());
    }
    build_claude_cached_lead_status_payload(
        state,
        event_name,
        pane_key,
        hook_payload,
        working_child_evidence,
        ended_child_work,
        ended_runtime_child_work,
        now,
    )
}

/// `buildClaudeCachedLeadStatusPayload` (`agent-hook-listener.ts:2882-2916`).
#[allow(clippy::too_many_arguments)] // mirrors the Electron helper's argument list
fn build_claude_cached_lead_status_payload(
    state: &mut HookListenerState,
    event_name: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
    working_child_evidence: bool,
    ended_child_work: bool,
    ended_runtime_child_work: bool,
    now: i64,
) -> Option<ParsedAgentStatusPayload> {
    let _ = (ended_runtime_child_work, now);
    let lead = state.claude_lead_state_by_pane_key.get(pane_key).cloned();
    let lead_state = if let Some(lead) = &lead {
        lead.state
    } else if working_child_evidence || ended_runtime_child_work || ended_child_work {
        // Why: ending a current-runtime child wakes its parent; only a cached
        // lead boundary can prove the whole pane completed.
        AgentStatusState::Working
    } else {
        return None;
    };
    let interrupted = state
        .claude_lead_state_by_pane_key
        .get(pane_key)
        .map(|lead| lead.interrupted)
        .unwrap_or(false);
    let turn_completed_at = state
        .claude_lead_state_by_pane_key
        .get(pane_key)
        .and_then(|lead| lead.turn_completed_at);
    let effective = resolve_claude_pane_state(state, pane_key, lead_state, interrupted);
    build_claude_status_payload(
        state,
        Some(event_name),
        "",
        pane_key,
        hook_payload,
        effective,
        false,
        Some(interrupted),
        None,
        turn_completed_at,
    )
}

/// `normalizeClaudeEvent` (`agent-hook-listener.ts:2918-3171`).
pub fn normalize_claude_event(
    state: &mut HookListenerState,
    event_name: Option<&str>,
    prompt_text: &str,
    pane_key: &str,
    hook_payload: &serde_json::Map<String, Value>,
) -> Option<ParsedAgentStatusPayload> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();
    let event_agent_id = read_string(hook_payload, "agent_id");
    if matches!(
        event_name,
        Some("SubagentStart") | Some("SubagentStop") | Some("TeammateIdle")
    ) {
        return normalize_claude_subagent_lifecycle_event(
            state,
            event_name.unwrap_or_default(),
            pane_key,
            hook_payload,
            now,
        );
    }
    if event_name == Some("SessionStart") {
        let session_start_source = hook_payload.get("source").and_then(Value::as_str);
        if event_agent_id.is_some()
            || !matches!(
                session_start_source,
                Some("startup") | Some("resume") | Some("clear")
            )
        {
            // Why: allowlist idle boundaries and fail closed — a compact
            // restart fires mid-turn and must not flip the lead's live turn.
            return None;
        }
        // Why: a new process owns the pane; stale children/tasks/crons must not
        // gate the fresh session's idle row back up to 'working'.
        state.claude_subagent_rosters.remove(pane_key);
        state
            .claude_running_non_agent_task_pane_keys
            .remove(pane_key);
        state.claude_active_session_cron_pane_keys.remove(pane_key);
        state.claude_lead_state_by_pane_key.insert(
            pane_key.to_string(),
            ClaudeLeadTurnState {
                state: AgentStatusState::Done,
                ..ClaudeLeadTurnState::default()
            },
        );
        return build_claude_status_payload(
            state,
            event_name,
            prompt_text,
            pane_key,
            hook_payload,
            AgentStatusState::Done,
            true,
            None,
            Some(true),
            None,
        );
    }
    let previous_lead = state.claude_lead_state_by_pane_key.get(pane_key).cloned();
    let is_turn_boundary = matches!(event_name, Some("Stop") | Some("StopFailure"));
    let interrupted = is_turn_boundary
        && ((event_agent_id.is_none()
            && hook_payload.get("is_interrupt").and_then(Value::as_bool) == Some(true))
            || previous_lead.as_ref().is_some_and(|lead| lead.interrupted));
    let background_tasks = read_claude_background_agent_tasks(hook_payload);
    let session_crons = hook_payload.get("session_crons").and_then(Value::as_array);
    let session_cron_inventory_present = session_crons.is_some();
    let has_active_session_cron =
        session_cron_inventory_present && session_crons.is_some_and(|crons| !crons.is_empty());

    if event_name == Some("UserPromptSubmit") && is_compact_continuation_user_turn_text(prompt_text)
    {
        // Why: the post-compact continuation prompt has no matching Stop and
        // would resurrect working.
        return None;
    }

    let event_tool_name = read_string(hook_payload, "tool_name");
    let is_ask_user_question_wait =
        matches!(event_name, Some("PreToolUse") | Some("PermissionRequest"))
            && is_ask_user_question_tool(event_tool_name.as_deref());
    let is_ask_user_question = event_name == Some("PreToolUse") && is_ask_user_question_wait;
    let reported_state_name: Option<AgentStatusState> = if matches!(
        event_name,
        Some("UserPromptSubmit")
            | Some("PostToolUse")
            | Some("PostToolUseFailure")
            | Some("PreCompact")
    ) || (event_name == Some("PostCompact")
        && hook_payload.get("trigger").and_then(Value::as_str) == Some("auto"))
        || (event_name == Some("PreToolUse") && !is_ask_user_question)
    {
        Some(AgentStatusState::Working)
    } else if event_name == Some("PermissionRequest") || is_ask_user_question {
        Some(AgentStatusState::Waiting)
    } else if is_turn_boundary
        || (event_name == Some("PostCompact")
            && hook_payload.get("trigger").and_then(Value::as_str) == Some("manual"))
    {
        Some(AgentStatusState::Done)
    } else {
        None
    };
    let reported_state_name = reported_state_name?;

    if background_tasks.present && event_agent_id.is_none() {
        update_claude_running_non_agent_task(
            state,
            pane_key,
            background_tasks.has_running_non_agent_task,
            interrupted,
        );
    }
    if session_cron_inventory_present && event_agent_id.is_none() {
        if has_active_session_cron && !interrupted {
            state
                .claude_active_session_cron_pane_keys
                .insert(pane_key.to_string());
        } else {
            state.claude_active_session_cron_pane_keys.remove(pane_key);
        }
    } else if event_agent_id.is_none() && is_turn_boundary && background_tasks.present {
        // Why: current Claude may omit an empty cron inventory while still
        // emitting background_tasks.
        state.claude_active_session_cron_pane_keys.remove(pane_key);
    }

    let event_tool_use_id = ["tool_use_id", "toolUseId"]
        .iter()
        .find_map(|key| read_string(hook_payload, key));
    let previous_tool = state.last_tool_by_pane_key.get(pane_key).cloned();
    let is_parallel_sibling_completion_during_question = event_agent_id.is_none()
        && previous_lead
            .as_ref()
            .is_some_and(|lead| lead.state == AgentStatusState::Waiting)
        && is_ask_user_question_tool(
            previous_tool
                .as_ref()
                .and_then(|tool| tool.tool_name.as_deref()),
        )
        && matches!(event_name, Some("PostToolUse") | Some("PostToolUseFailure"))
        && previous_lead
            .as_ref()
            .is_some_and(|lead| lead.waiting_tool_use_id.is_some())
        && event_tool_use_id.is_some()
        && previous_lead.as_ref().is_some_and(|lead| {
            event_tool_use_id.as_deref() != lead.waiting_tool_use_id.as_deref()
        });
    if is_parallel_sibling_completion_during_question {
        return build_claude_cached_lead_status_payload(
            state,
            event_name.unwrap_or_default(),
            pane_key,
            hook_payload,
            false,
            false,
            false,
            now,
        );
    }

    // Why: subagent/teammate events carry `agent_id` (lead's don't); child tool
    // activity keeps its row live but must not become the lead's state.
    let is_waiting_inducing = reported_state_name == AgentStatusState::Waiting;
    let subagent_origin_id = if !is_waiting_inducing
        && matches!(
            event_name,
            Some("PreToolUse") | Some("PostToolUse") | Some("PostToolUseFailure")
        ) {
        event_agent_id.clone()
    } else {
        None
    };
    if event_agent_id.is_some() && (subagent_origin_id.is_some() || is_waiting_inducing) {
        let roster = get_or_create_roster(&mut state.claude_subagent_rosters, pane_key);
        upsert_working_claude_subagent(
            roster,
            event_agent_id.as_deref().unwrap_or_default(),
            (&read_string(hook_payload, "agent_type"), &None),
            now,
        );
    }
    if let Some(subagent_origin_id) = &subagent_origin_id {
        let lead = state.claude_lead_state_by_pane_key.get(pane_key).cloned();
        if !lead.as_ref().is_some_and(|lead| {
            lead.state == AgentStatusState::Waiting
                && lead.waiting_agent_id.as_deref() == Some(subagent_origin_id.as_str())
        }) {
            let working_evidence = claude_roster::claude_roster_has_runtime_working_subagent(
                state.claude_subagent_rosters.get(pane_key),
            );
            return build_claude_cached_lead_status_payload(
                state,
                event_name.unwrap_or_default(),
                pane_key,
                hook_payload,
                working_evidence,
                false,
                false,
                now,
            );
        }
        let is_parallel_sibling_completion_during_child_question =
            matches!(event_name, Some("PostToolUse") | Some("PostToolUseFailure"))
                && lead
                    .as_ref()
                    .is_some_and(|lead| lead.waiting_tool_use_id.is_some())
                && event_tool_use_id.is_some()
                && lead.as_ref().is_some_and(|lead| {
                    event_tool_use_id.as_deref() != lead.waiting_tool_use_id.as_deref()
                });
        if is_parallel_sibling_completion_during_child_question {
            let working_evidence = claude_roster::claude_roster_has_runtime_working_subagent(
                state.claude_subagent_rosters.get(pane_key),
            );
            return build_claude_cached_lead_status_payload(
                state,
                event_name.unwrap_or_default(),
                pane_key,
                hook_payload,
                working_evidence,
                false,
                false,
                now,
            );
        }
        // Why: approval granted — restore the stashed lead state, not this
        // child's 'working': the lead may already be done, and the done-gate
        // never upgrades working back to done once the roster drains.
        let restored = lead
            .as_ref()
            .and_then(|lead| lead.state_before_wait.clone())
            .unwrap_or(StashedLeadState {
                state: AgentStatusState::Working,
                ..Default::default()
            });
        state.claude_lead_state_by_pane_key.insert(
            pane_key.to_string(),
            ClaudeLeadTurnState {
                state: restored.state,
                interrupted: restored.interrupted,
                turn_completed_at: restored.turn_completed_at,
                ..ClaudeLeadTurnState::default()
            },
        );
        let effective =
            resolve_claude_pane_state(state, pane_key, restored.state, restored.interrupted);
        return build_claude_status_payload(
            state,
            event_name,
            prompt_text,
            pane_key,
            hook_payload,
            effective,
            true,
            Some(restored.interrupted),
            None,
            restored.turn_completed_at,
        );
    }

    // Why: lead events never carry agent_id; even a child missed by lifecycle
    // tracking cannot own the lead turn or its background-work evidence.
    if event_agent_id.is_some() && !is_waiting_inducing {
        let working_evidence = claude_roster::claude_roster_has_runtime_working_subagent(
            state.claude_subagent_rosters.get(pane_key),
        );
        return build_claude_cached_lead_status_payload(
            state,
            event_name.unwrap_or_default(),
            pane_key,
            hook_payload,
            working_evidence,
            false,
            false,
            now,
        );
    }

    if is_turn_boundary && event_agent_id.is_none() && background_tasks.present {
        let mut rosters = std::collections::HashMap::new();
        rosters.insert(
            pane_key.to_string(),
            state
                .claude_subagent_rosters
                .remove(pane_key)
                .unwrap_or_default(),
        );
        {
            let roster = claude_roster::get_or_create_roster(&mut rosters, pane_key);
            fold_claude_background_tasks_into_roster(
                roster,
                &background_tasks.tasks,
                now,
                !background_tasks.truncated,
            );
        }
        state
            .claude_subagent_rosters
            .insert(pane_key.to_string(), rosters.remove(pane_key).unwrap());
    }

    // Why: a child-induced wait displaces the lead state; stash it so clearing
    // restores reality (lead may be done). A 2nd child wait carries the
    // ORIGINAL stash, not the intermediate waiting state.
    let state_before_wait: Option<StashedLeadState> =
        if is_waiting_inducing && event_agent_id.is_some() {
            previous_lead.as_ref().map(|lead| {
                if lead.state == AgentStatusState::Waiting {
                    lead.state_before_wait.clone().unwrap_or(StashedLeadState {
                        state: AgentStatusState::Working,
                        ..Default::default()
                    })
                } else {
                    StashedLeadState {
                        state: lead.state,
                        interrupted: lead.interrupted,
                        turn_completed_at: lead.turn_completed_at,
                    }
                }
            })
        } else {
            None
        };
    let waiting_tool_use_id = event_tool_use_id.clone().or_else(|| {
        previous_lead
            .as_ref()
            .and_then(|lead| lead.waiting_tool_use_id.clone())
    });

    if interrupted && event_agent_id.is_none() {
        state
            .claude_running_non_agent_task_pane_keys
            .remove(pane_key);
        state.claude_active_session_cron_pane_keys.remove(pane_key);
    }

    let effective_state =
        resolve_claude_pane_state(state, pane_key, reported_state_name, interrupted);
    // Why: the lead already ended — the pane stays `working` only because
    // background inventory is still registered; this end time is the per-turn
    // identity and the later all-clear's pair key.
    let turn_completed_at: Option<i64> = if event_agent_id.is_none()
        && is_turn_boundary
        && reported_state_name == AgentStatusState::Done
        && effective_state == AgentStatusState::Working
        && !interrupted
    {
        Some(now)
    } else {
        None
    };

    state.claude_lead_state_by_pane_key.insert(
        pane_key.to_string(),
        ClaudeLeadTurnState {
            state: reported_state_name,
            interrupted,
            waiting_agent_id: if is_waiting_inducing && event_agent_id.is_some() {
                event_agent_id.clone()
            } else {
                None
            },
            waiting_tool_use_id: if is_ask_user_question_wait {
                waiting_tool_use_id
            } else {
                None
            },
            turn_completed_at,
            state_before_wait,
        },
    );

    let effective_roster = state.claude_subagent_rosters.get(pane_key);
    if is_turn_boundary
        && event_agent_id.is_none()
        && effective_state == AgentStatusState::Working
        && claude_roster::claude_roster_has_restored_snapshot_subagent(effective_roster)
        && !claude_roster::claude_roster_has_runtime_working_subagent(effective_roster)
        && !state
            .claude_running_non_agent_task_pane_keys
            .contains(pane_key)
        && !state
            .claude_active_session_cron_pane_keys
            .contains(pane_key)
    {
        // Why: a legacy or partial Stop confirms the lead boundary, not a child
        // restored from disk; keep the child-only gate eligible for reconciliation.
        state
            .claude_unconfirmed_restored_status_pane_keys
            .insert(pane_key.to_string());
    }

    build_claude_status_payload(
        state,
        event_name,
        prompt_text,
        pane_key,
        hook_payload,
        effective_state,
        true,
        Some(interrupted),
        None,
        turn_completed_at,
    )
}

/// `seedClaudeLeadTurnFromPersistedStatus` (`agent-hook-listener.ts:2787-2810`).
pub fn seed_claude_lead_turn_from_persisted_status(
    state: &mut HookListenerState,
    pane_key: &str,
    payload: &ParsedAgentStatusPayload,
    child_only_boundary: bool,
) {
    if child_only_boundary && payload.agent_type.as_deref() == Some("claude") {
        state.claude_lead_state_by_pane_key.insert(
            pane_key.to_string(),
            ClaudeLeadTurnState {
                state: AgentStatusState::Done,
                interrupted: payload.interrupted,
                turn_completed_at: payload.turn_completed_at,
                ..ClaudeLeadTurnState::default()
            },
        );
        if !payload.prompt.is_empty() {
            state
                .last_prompt_by_pane_key
                .insert(pane_key.to_string(), payload.prompt.clone());
        }
        if let Some(last_assistant_message) = &payload.last_assistant_message {
            state.last_tool_by_pane_key.insert(
                pane_key.to_string(),
                ToolSnapshot {
                    last_assistant_message: Some(last_assistant_message.clone()),
                    ..ToolSnapshot::default()
                },
            );
        }
    }
}
