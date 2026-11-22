//! Hook-driven Codex children have their own model and can wait independently.

use std::collections::HashMap;

use crate::normalizers::fields::{
    AGENT_MODEL_MAX_LENGTH, AGENT_STATUS_MAX_SUBAGENTS, AGENT_TYPE_MAX_LENGTH,
    normalize_optional_field,
};
use crate::types::{AgentStatusState, AgentSubagentSnapshot, AgentSubagentState};

pub type CodexSubagentRoster = HashMap<String, AgentSubagentSnapshot>;

pub fn upsert(
    roster: &mut CodexSubagentRoster,
    id: &str,
    agent_type: Option<&str>,
    model: Option<&str>,
    state: AgentSubagentState,
    now: i64,
) {
    let id = id.trim();
    if id.is_empty() || id.encode_utf16().count() > 64 {
        return;
    }
    let agent_type = normalize_optional_field(agent_type, AGENT_TYPE_MAX_LENGTH);
    let model = normalize_optional_field(model, AGENT_MODEL_MAX_LENGTH);
    if let Some(existing) = roster.get_mut(id) {
        existing.agent_type = agent_type.or_else(|| existing.agent_type.clone());
        existing.model = model.or_else(|| existing.model.clone());
        existing.state = state;
        return;
    }
    if roster.len() < AGENT_STATUS_MAX_SUBAGENTS {
        roster.insert(
            id.to_string(),
            AgentSubagentSnapshot {
                id: id.to_string(),
                agent_type,
                model,
                state,
                started_at: now,
                description: None,
            },
        );
    }
}

pub fn snapshots(roster: Option<&CodexSubagentRoster>) -> Option<Vec<AgentSubagentSnapshot>> {
    let roster = roster.filter(|roster| !roster.is_empty())?;
    let mut snapshots: Vec<_> = roster.values().cloned().collect();
    snapshots.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Some(snapshots)
}

pub fn effective_state(
    roster: Option<&CodexSubagentRoster>,
    lead: AgentStatusState,
) -> AgentStatusState {
    let Some(roster) = roster.filter(|roster| !roster.is_empty()) else {
        return lead;
    };
    if roster
        .values()
        .any(|child| child.state == AgentSubagentState::Waiting)
    {
        AgentStatusState::Waiting
    } else if lead == AgentStatusState::Done {
        AgentStatusState::Working
    } else {
        lead
    }
}
