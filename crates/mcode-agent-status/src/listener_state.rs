//! Per-listener caches — Rust port of `HookListenerState`
//! (`agent-hook-listener.ts:127-259`) + prompt resolution (`:650-668`) +
//! lead/codex turn-state structs.

use crate::normalizers::claude_roster::ClaudeSubagentRoster;
use crate::normalizers::codex_roster::CodexSubagentRoster;
use crate::normalizers::harness_text::is_known_harness_injected_user_turn_text;
use crate::normalizers::tool_snapshot::ToolSnapshot;
use crate::types::{AgentStatusState, ParsedAgentStatusPayload};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClaudeLeadTurnState {
    pub state: AgentStatusState,
    pub interrupted: bool,
    /// Subagent that induced the wait; only its next tool activity may clear it.
    pub waiting_agent_id: Option<String>,
    /// Tool call that owns the wait; late completions from parallel siblings
    /// must not dismiss its card.
    pub waiting_tool_use_id: Option<String>,
    /// End time of the lead turn closed while background inventory kept the
    /// pane `working`. Repeated on the later all-clear `done`.
    pub turn_completed_at: Option<i64>,
    /// Lead state a child-induced wait displaced, restored when the wait clears.
    pub state_before_wait: Option<StashedLeadState>,
}

/// `Pick<ClaudeLeadTurnState, 'state' | 'interrupted' | 'turnCompletedAt'>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StashedLeadState {
    pub state: AgentStatusState,
    pub interrupted: bool,
    pub turn_completed_at: Option<i64>,
}

impl StashedLeadState {
    pub fn from_lead(lead: &ClaudeLeadTurnState) -> Self {
        Self {
            state: lead.state,
            interrupted: lead.interrupted,
            turn_completed_at: lead.turn_completed_at,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodexLeadTurnState {
    pub state: AgentStatusState,
    pub model: Option<String>,
}

/// Per-listener-instance caches needing per-PTY teardown; MCode's main process
/// and a relay each get their own, never shared.
#[derive(Clone, Debug, Default)]
pub struct HookListenerState {
    pub warned_versions: std::collections::HashSet<String>,
    pub warned_envs: std::collections::HashSet<String>,
    pub last_prompt_by_pane_key: std::collections::HashMap<String, String>,
    pub last_tool_by_pane_key: std::collections::HashMap<String, ToolSnapshot>,
    pub antigravity_completed_transcript_by_pane_key: std::collections::HashMap<String, String>,
    pub amp_completed_cache_keys: std::collections::HashSet<String>,
    pub amp_cache_order: crate::normalizers::amp::AmpCacheOrder,
    pub claude_subagent_rosters: std::collections::HashMap<String, ClaudeSubagentRoster>,
    pub claude_lead_state_by_pane_key: std::collections::HashMap<String, ClaudeLeadTurnState>,
    pub claude_unconfirmed_restored_status_pane_keys: std::collections::HashSet<String>,
    pub claude_running_non_agent_task_pane_keys: std::collections::HashSet<String>,
    pub claude_active_session_cron_pane_keys: std::collections::HashSet<String>,
    pub codex_subagent_rosters: std::collections::HashMap<String, CodexSubagentRoster>,
    pub codex_lead_state_by_pane_key: std::collections::HashMap<String, CodexLeadTurnState>,
    pub last_status_by_pane_key:
        std::collections::HashMap<String, crate::types::AgentHookEventPayload>,
}

/// `clearPaneTurnCacheState` (`agent-hook-listener.ts:254-259`).
pub fn clear_pane_turn_cache_state(state: &mut HookListenerState, pane_key: &str) {
    state.last_prompt_by_pane_key.remove(pane_key);
    state.last_tool_by_pane_key.remove(pane_key);
}

/// `clearPaneCacheState` (`agent-hook-listener.ts:191-204`).
pub fn clear_pane_cache_state(state: &mut HookListenerState, pane_key: &str) {
    let prefix = format!("{pane_key}\0");
    state
        .last_prompt_by_pane_key
        .retain(|key, _| !key.starts_with(&prefix));
    state
        .last_tool_by_pane_key
        .retain(|key, _| !key.starts_with(&prefix));
    state
        .amp_completed_cache_keys
        .retain(|key| key != pane_key && !key.starts_with(&prefix));
    state.amp_cache_order.remove_pane(pane_key);
    state.last_prompt_by_pane_key.remove(pane_key);
    state.last_tool_by_pane_key.remove(pane_key);
    state
        .antigravity_completed_transcript_by_pane_key
        .remove(pane_key);
    state.claude_subagent_rosters.remove(pane_key);
    state.claude_lead_state_by_pane_key.remove(pane_key);
    state
        .claude_unconfirmed_restored_status_pane_keys
        .remove(pane_key);
    state
        .claude_running_non_agent_task_pane_keys
        .remove(pane_key);
    state.claude_active_session_cron_pane_keys.remove(pane_key);
    state.codex_subagent_rosters.remove(pane_key);
    state.codex_lead_state_by_pane_key.remove(pane_key);
}

/// `clearAllListenerCaches` (`agent-hook-listener.ts:281-298`).
pub fn clear_all_listener_caches(state: &mut HookListenerState) {
    state.amp_completed_cache_keys.clear();
    state.amp_cache_order = Default::default();
    state.last_prompt_by_pane_key.clear();
    state.last_tool_by_pane_key.clear();
    state.antigravity_completed_transcript_by_pane_key.clear();
    state.claude_subagent_rosters.clear();
    state.claude_lead_state_by_pane_key.clear();
    state.claude_unconfirmed_restored_status_pane_keys.clear();
    state.claude_running_non_agent_task_pane_keys.clear();
    state.claude_active_session_cron_pane_keys.clear();
    state.codex_subagent_rosters.clear();
    state.codex_lead_state_by_pane_key.clear();
}

/// `resolvePrompt` (`agent-hook-listener.ts:650-668`).
pub fn resolve_prompt(
    state: &mut HookListenerState,
    pane_key: &str,
    prompt_text: &str,
    reset_on_new_turn: bool,
) -> String {
    // Why: harness-injected turns fire UserPromptSubmit but aren't the user's
    // ask — keep cached prompt; match only known tags so real <tags> still reset.
    if is_known_harness_injected_user_turn_text(prompt_text) {
        return state
            .last_prompt_by_pane_key
            .get(pane_key)
            .cloned()
            .unwrap_or_default();
    }
    if reset_on_new_turn {
        state.last_prompt_by_pane_key.remove(pane_key);
    }
    if !prompt_text.is_empty() {
        state
            .last_prompt_by_pane_key
            .insert(pane_key.to_string(), prompt_text.to_string());
        return prompt_text.to_string();
    }
    state
        .last_prompt_by_pane_key
        .get(pane_key)
        .cloned()
        .unwrap_or_default()
}

/// `agent-hook-listener.ts:414-415` — the normalizer clamps `interrupted` to
/// done payloads; preserve in lead state for the eventual done.
pub fn payload_interrupted(payload: &ParsedAgentStatusPayload) -> bool {
    payload.interrupted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_turns_keep_cached_prompt() {
        let mut state = HookListenerState::default();
        let first = resolve_prompt(&mut state, "p", "real ask", false);
        assert_eq!(first, "real ask");
        let second = resolve_prompt(&mut state, "p", "<system-reminder>noise", false);
        assert_eq!(second, "real ask");
        let third = resolve_prompt(&mut state, "p", "next ask", true);
        assert_eq!(third, "next ask");
    }

    #[test]
    fn clear_pane_cache_wipes_scoped_state() {
        let mut state = HookListenerState::default();
        state.claude_lead_state_by_pane_key.insert(
            "p".into(),
            ClaudeLeadTurnState {
                state: AgentStatusState::Working,
                ..Default::default()
            },
        );
        clear_pane_cache_state(&mut state, "p");
        assert!(!state.claude_lead_state_by_pane_key.contains_key("p"));
    }
}
