//! Claude subagent roster — Rust port of `src/shared/claude-subagent-roster.ts`
//! (356 ln) + `src/shared/claude-background-task-inventory.ts` (97 ln).
//! Every guard comment in the original is a shipped bugfix; ported verbatim.

use serde_json::Value;
use std::collections::HashMap;

use crate::normalizers::fields::normalize_optional_field;
use crate::normalizers::fields::{
    AGENT_STATUS_MAX_SUBAGENTS, AGENT_TYPE_MAX_LENGTH, TOOL_INPUT_MAX_LENGTH,
};
use crate::types::{AgentSubagentSnapshot, AgentSubagentState};

/// Mirrors the wire-normalization id cap; enforced at upsert so an over-long
/// id can't gate the pane 'working' while being invisible in emitted snapshots.
const CLAUDE_SUBAGENT_ID_MAX_LENGTH: usize = 64;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackedClaudeSubagent {
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub started_at: i64,
    /// 'idle' = teammate between mailbox turns: alive/resumable, row stays
    /// visible but must not gate the pane 'working'.
    pub state: TrackedState,
    /// A TeammateIdle matched this id by name — proof it is a persistent
    /// in-process teammate. Never cleared: identity can't change mid-life.
    pub confirmed_teammate: bool,
    /// The id came from a persisted snapshot or background_tasks, not live
    /// lifecycle events; may be a phantom whose SubagentStop was never observed.
    pub background_tasks_authoritative: bool,
    /// The row was rebuilt from a persisted snapshot and no live event has
    /// confirmed it since. Cleared by any live activity on the id.
    pub restored_from_snapshot: bool,
    /// A subagent-typed background task listed this lifecycle id id-exact.
    pub listed_as_subagent_task: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrackedState {
    #[default]
    Working,
    Idle,
}

pub type ClaudeSubagentRoster = HashMap<String, TrackedClaudeSubagent>;

/// One agent entry from the `background_tasks` array.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaudeBackgroundAgentTask {
    pub id: String,
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub running: bool,
    /// True for `type: "teammate"` entries: ids never match lifecycle agent_ids
    /// and they report "running" permanently — no per-agent state at all.
    pub teammate: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackgroundTasksInventory {
    pub present: bool,
    pub tasks: Vec<ClaudeBackgroundAgentTask>,
    pub truncated: bool,
    pub has_running_non_agent_task: bool,
}

const CLAUDE_TERMINAL_BACKGROUND_TASK_STATUSES: &[&str] = &[
    "idle",
    "done",
    "success",
    "succeeded",
    "complete",
    "completed",
    "finished",
    "failed",
    "error",
    "terminated",
    "exited",
    "aborted",
    "expired",
    "skipped",
    "crashed",
    "killed",
    "cancelled",
    "canceled",
    "timed_out",
];

/// `readClaudeBackgroundAgentTasks` (`claude-background-task-inventory.ts:31-97`).
/// `present: false` means the field was absent/malformed (older builds), so
/// callers must keep their tracked roster instead of clearing it.
pub fn read_claude_background_agent_tasks(
    payload: &serde_json::Map<String, Value>,
) -> BackgroundTasksInventory {
    let Some(Value::Array(raw)) = payload.get("background_tasks") else {
        return BackgroundTasksInventory::default();
    };
    let mut out = BackgroundTasksInventory {
        present: true,
        ..Default::default()
    };
    for item in raw {
        let Some(obj) = item.as_object() else {
            out.truncated = true;
            out.has_running_non_agent_task = true;
            continue;
        };
        let task_type = obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let task_status = obj
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let is_terminal = !task_status.is_empty()
            && CLAUDE_TERMINAL_BACKGROUND_TASK_STATUSES.contains(&task_status.as_str());
        if task_type.is_empty() {
            out.truncated = true;
            out.has_running_non_agent_task |= !is_terminal;
            continue;
        }
        let is_agent_task = task_type == "subagent" || task_type == "teammate";
        // Why: future non-agent types and nonterminal labels must fail active;
        // only typed agent rows or explicit terminal states can safely retire work.
        if !is_agent_task && !is_terminal {
            out.has_running_non_agent_task = true;
        }
        if !is_agent_task {
            continue;
        }
        let Some(id) = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            out.truncated = true;
            continue;
        };
        if out.tasks.len() >= AGENT_STATUS_MAX_SUBAGENTS {
            // A capped inventory cannot prove a tracked id is absent; callers
            // must retain unlisted rows rather than deleting live overflow tasks.
            out.truncated = true;
            continue;
        }
        out.tasks.push(ClaudeBackgroundAgentTask {
            id: id.to_string(),
            agent_type: obj
                .get("agent_type")
                .and_then(Value::as_str)
                .map(str::to_string),
            description: obj
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            running: !is_terminal,
            teammate: task_type == "teammate",
        });
    }
    out
}

/// Agent-team/named-agent lifecycle ids are `a<name>-<hex>` while one-shot ids
/// are hyphen-free (`a<hex>`).
pub fn is_claude_teammate_lifecycle_id(id: &str) -> bool {
    let Some(separator) = id.rfind('-') else {
        return false;
    };
    separator > 1
        && id.starts_with('a')
        && id[separator + 1..].bytes().all(|b| b.is_ascii_hexdigit())
        && !id[separator + 1..].is_empty()
}

/// Teammate ids embed the name as `a<name>-<hex>`; requiring a hyphen-free
/// suffix keeps teammate "rev" from matching "rev-two"'s ids.
pub fn claude_teammate_id_matches_name(id: &str, name: &str) -> bool {
    let prefix = format!("a{name}-");
    match id.strip_prefix(&prefix) {
        Some(rest) => !rest.contains('-'),
        None => false,
    }
}

pub fn get_or_create_roster<'a>(
    rosters: &'a mut HashMap<String, ClaudeSubagentRoster>,
    pane_key: &str,
) -> &'a mut ClaudeSubagentRoster {
    rosters.entry(pane_key.to_string()).or_default()
}

/// `upsertWorkingClaudeSubagent` (`claude-subagent-roster.ts:63-100`).
pub fn upsert_working_claude_subagent(
    roster: &mut ClaudeSubagentRoster,
    id: &str,
    fields: (&Option<String>, &Option<String>),
    now: i64,
) {
    if id.is_empty() || id.len() > CLAUDE_SUBAGENT_ID_MAX_LENGTH {
        return;
    }
    if let Some(existing) = roster.get_mut(id) {
        existing.state = TrackedState::Working;
        existing.agent_type = fields.0.clone().or_else(|| existing.agent_type.clone());
        existing.description = fields.1.clone().or_else(|| existing.description.clone());
        // Live activity proves the lifecycle stream owns this id again.
        existing.background_tasks_authoritative = false;
        existing.restored_from_snapshot = false;
        return;
    }
    // Beyond the wire cap extra rows would be invisible anyway; idle teammates
    // are the only safe eviction — never displace a working child.
    if roster.len() >= AGENT_STATUS_MAX_SUBAGENTS && !evict_oldest_idle(roster) {
        return;
    }
    roster.insert(
        id.to_string(),
        TrackedClaudeSubagent {
            agent_type: fields.0.clone(),
            description: fields.1.clone(),
            started_at: now,
            state: TrackedState::Working,
            ..TrackedClaudeSubagent::default()
        },
    );
}

fn evict_oldest_idle(roster: &mut ClaudeSubagentRoster) -> bool {
    let oldest = roster
        .iter()
        .filter(|(_, tracked)| tracked.state == TrackedState::Idle)
        .min_by_key(|(_, tracked)| tracked.started_at)
        .map(|(id, _)| id.clone());
    match oldest {
        Some(id) => roster.remove(&id).is_some(),
        None => false,
    }
}

/// `stopClaudeSubagent` (`claude-subagent-roster.ts:120-137`).
pub fn stop_claude_subagent(roster: &mut ClaudeSubagentRoster, id: &str) {
    let Some(tracked) = roster.get_mut(id) else {
        return;
    };
    if !is_claude_teammate_lifecycle_id(id) || tracked.listed_as_subagent_task {
        roster.remove(id);
        return;
    }
    tracked.background_tasks_authoritative = false;
    tracked.restored_from_snapshot = false;
    tracked.state = TrackedState::Idle;
}

/// `idleClaudeTeammateByName` (`claude-subagent-roster.ts:292-304`).
pub fn idle_claude_teammate_by_name(roster: &mut ClaudeSubagentRoster, name: &str) -> bool {
    let mut changed = false;
    let matching: Vec<String> = roster
        .keys()
        .filter(|id| claude_teammate_id_matches_name(id, name))
        .cloned()
        .collect();
    for id in matching {
        if let Some(tracked) = roster.get_mut(&id) {
            changed |= tracked.state != TrackedState::Idle || !tracked.confirmed_teammate;
            tracked.background_tasks_authoritative = false;
            tracked.restored_from_snapshot = false;
            tracked.state = TrackedState::Idle;
            tracked.confirmed_teammate = true;
        }
    }
    changed
}

/// `foldClaudeBackgroundTasksIntoRoster` (`claude-subagent-roster.ts:154-250`).
pub fn fold_claude_background_tasks_into_roster(
    roster: &mut ClaudeSubagentRoster,
    tasks: &[ClaudeBackgroundAgentTask],
    now: i64,
    inventory_complete: bool,
) {
    if tasks.is_empty() {
        if inventory_complete {
            roster.clear();
        }
        return;
    }
    let mut listed_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pending_running_tasks: Vec<&ClaudeBackgroundAgentTask> = Vec::new();
    let has_teammate_typed_task = tasks.iter().any(|task| task.teammate);
    for task in tasks {
        if task.teammate {
            continue;
        }
        listed_ids.insert(task.id.clone());
        if let Some(existing) = roster.get_mut(&task.id) {
            if !task.running {
                let id = task.id.clone();
                pending_running_tasks.retain(|t| t.id != id);
                roster.remove(&id);
                continue;
            }
            // A Stop can park the row before the lead inventory confirms the
            // same workflow lane is still running; the authoritative task wins.
            existing.state = TrackedState::Working;
            existing.agent_type = task
                .agent_type
                .clone()
                .or_else(|| existing.agent_type.clone());
            existing.description = task
                .description
                .clone()
                .or_else(|| existing.description.clone());
            existing.listed_as_subagent_task = true;
            existing.restored_from_snapshot = false;
            continue;
        }
        if !task.running {
            continue;
        }
        upsert_working_claude_subagent(
            roster,
            &task.id,
            (&task.agent_type, &task.description),
            now,
        );
        match roster.get_mut(&task.id) {
            Some(created) => {
                created.background_tasks_authoritative = true;
                created.listed_as_subagent_task = true;
            }
            // A full roster may still contain stale entries that this same
            // inventory will reap. Retry after cleanup so a replacement stays live.
            None => pending_running_tasks.push(task),
        }
    }
    if inventory_complete {
        let reaped: Vec<String> = roster
            .iter()
            .filter(|(id, tracked)| {
                if listed_ids.contains(*id) {
                    return false;
                }
                if has_teammate_typed_task
                    && !tracked.background_tasks_authoritative
                    && !tracked.listed_as_subagent_task
                    && is_claude_teammate_lifecycle_id(id)
                    && (tracked.state == TrackedState::Working || tracked.confirmed_teammate)
                {
                    return false;
                }
                true
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in reaped {
            roster.remove(&id);
        }
    }
    for task in pending_running_tasks {
        if roster.len() >= AGENT_STATUS_MAX_SUBAGENTS {
            break;
        }
        upsert_working_claude_subagent(
            roster,
            &task.id,
            (&task.agent_type, &task.description),
            now,
        );
        if let Some(created) = roster.get_mut(&task.id) {
            created.background_tasks_authoritative = true;
            created.listed_as_subagent_task = true;
        }
    }
}

/// Drop restored rows that no current-runtime child activity has confirmed.
pub fn reap_unconfirmed_restored_claude_subagents(roster: &mut ClaudeSubagentRoster) -> bool {
    let reaped: Vec<String> = roster
        .iter()
        .filter(|(_, tracked)| tracked.restored_from_snapshot)
        .map(|(id, _)| id.clone())
        .collect();
    let changed = !reaped.is_empty();
    for id in reaped {
        roster.remove(&id);
    }
    changed
}

pub fn claude_roster_has_working_subagent(roster: Option<&ClaudeSubagentRoster>) -> bool {
    roster.is_some_and(|roster| {
        roster
            .values()
            .any(|tracked| tracked.state == TrackedState::Working)
    })
}

/// A working child observed in this listener runtime, not merely restored.
pub fn claude_roster_has_runtime_working_subagent(roster: Option<&ClaudeSubagentRoster>) -> bool {
    roster.is_some_and(|roster| {
        roster.values().any(|tracked| {
            tracked.state == TrackedState::Working && !tracked.restored_from_snapshot
        })
    })
}

pub fn claude_roster_has_restored_snapshot_subagent(roster: Option<&ClaudeSubagentRoster>) -> bool {
    roster.is_some_and(|roster| {
        roster
            .values()
            .any(|tracked| tracked.restored_from_snapshot)
    })
}

/// `claudeRosterToSnapshots` — hook arrival order is not stable across
/// reconciles; sort so equal rosters serialize identically.
pub fn claude_roster_to_snapshots(
    roster: Option<&ClaudeSubagentRoster>,
) -> Option<Vec<AgentSubagentSnapshot>> {
    let roster = roster?;
    if roster.is_empty() {
        return None;
    }
    let mut snapshots: Vec<AgentSubagentSnapshot> = roster
        .iter()
        .map(|(id, tracked)| AgentSubagentSnapshot {
            id: id.clone(),
            state: match tracked.state {
                TrackedState::Working => AgentSubagentState::Working,
                TrackedState::Idle => AgentSubagentState::Idle,
            },
            started_at: tracked.started_at,
            agent_type: normalize_optional_field(
                tracked.agent_type.as_deref(),
                AGENT_TYPE_MAX_LENGTH,
            ),
            model: None,
            description: normalize_optional_field(
                tracked.description.as_deref(),
                TOOL_INPUT_MAX_LENGTH,
            ),
        })
        .collect();
    snapshots.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Some(snapshots)
}

/// `seedClaudeSubagentRosterFromSnapshots` (`agent-hook-listener.ts:2760-2785`).
pub fn seed_claude_subagent_roster_from_snapshots(
    rosters: &mut HashMap<String, ClaudeSubagentRoster>,
    pane_key: &str,
    snapshots: &[AgentSubagentSnapshot],
) {
    if snapshots.is_empty() || rosters.contains_key(pane_key) {
        return;
    }
    let roster = get_or_create_roster(rosters, pane_key);
    for snapshot in snapshots {
        // Idle-teammate liveness can't be proven across a restart; only working
        // seeds restore, and a live teammate re-earns its row via SubagentStart.
        if snapshot.state != AgentSubagentState::Working {
            continue;
        }
        roster.insert(
            snapshot.id.clone(),
            TrackedClaudeSubagent {
                state: TrackedState::Working,
                started_at: snapshot.started_at,
                agent_type: snapshot.agent_type.clone(),
                description: snapshot.description.clone(),
                background_tasks_authoritative: true,
                restored_from_snapshot: true,
                ..TrackedClaudeSubagent::default()
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn inventory(payload: Value) -> BackgroundTasksInventory {
        read_claude_background_agent_tasks(payload.as_object().unwrap())
    }

    #[test]
    fn absent_list_is_not_present() {
        let inv = inventory(json!({}));
        assert!(!inv.present);
        let inv = inventory(json!({"background_tasks": "nope"}));
        assert!(!inv.present);
    }

    #[test]
    fn agent_tasks_read_teammates_skipped() {
        let inv = inventory(json!({"background_tasks": [
            {"id":"a1","type":"subagent","status":"running","agent_type":"code"},
            {"id":"t1","type":"teammate","status":"running"},
            {"type":"shell","status":"running"},
            {"type":"shell","status":"completed"}
        ]}));
        assert!(inv.present);
        assert_eq!(inv.tasks.len(), 2);
        assert!(!inv.tasks[0].teammate);
        assert!(inv.tasks[0].running);
        assert!(inv.tasks[1].teammate);
        assert!(inv.has_running_non_agent_task);
    }

    #[test]
    fn one_shot_vs_teammate_shape() {
        assert!(is_claude_teammate_lifecycle_id("arev-0a1b"));
        assert!(!is_claude_teammate_lifecycle_id("a0a1b"));
        assert!(!is_claude_teammate_lifecycle_id("arev"));
    }

    #[test]
    fn stop_removes_one_shot_parks_teammate() {
        let mut roster = ClaudeSubagentRoster::new();
        upsert_working_claude_subagent(&mut roster, "a0a1b", (&None, &None), 1);
        stop_claude_subagent(&mut roster, "a0a1b");
        assert!(roster.is_empty());
        upsert_working_claude_subagent(&mut roster, "arev-0a1b", (&None, &None), 2);
        stop_claude_subagent(&mut roster, "arev-0a1b");
        assert_eq!(roster["arev-0a1b"].state, TrackedState::Idle);
    }

    #[test]
    fn fold_clears_on_empty_complete_inventory() {
        let mut roster = ClaudeSubagentRoster::new();
        upsert_working_claude_subagent(&mut roster, "a0a1b", (&None, &None), 1);
        fold_claude_background_tasks_into_roster(&mut roster, &[], 2, true);
        assert!(roster.is_empty());
    }

    #[test]
    fn fold_keeps_confirmed_teammate_when_teammate_typed_present() {
        let mut roster = ClaudeSubagentRoster::new();
        upsert_working_claude_subagent(&mut roster, "arev-0a1b", (&None, &None), 1);
        idle_claude_teammate_by_name(&mut roster, "rev");
        assert!(roster["arev-0a1b"].confirmed_teammate);
        let tasks = vec![ClaudeBackgroundAgentTask {
            id: "zzz".into(),
            agent_type: None,
            description: None,
            running: true,
            teammate: true,
        }];
        fold_claude_background_tasks_into_roster(&mut roster, &tasks, 2, true);
        assert!(roster.contains_key("arev-0a1b"));
    }

    #[test]
    fn fold_removes_unlisted_one_shot() {
        let mut roster = ClaudeSubagentRoster::new();
        upsert_working_claude_subagent(&mut roster, "a0a1b", (&None, &None), 1);
        let tasks = vec![ClaudeBackgroundAgentTask {
            id: "a0a1b".into(),
            agent_type: None,
            description: None,
            running: false,
            teammate: false,
        }];
        fold_claude_background_tasks_into_roster(&mut roster, &tasks, 2, true);
        assert!(roster.is_empty());
    }

    #[test]
    fn snapshots_sorted_by_started_at() {
        let mut roster = ClaudeSubagentRoster::new();
        upsert_working_claude_subagent(&mut roster, "b", (&None, &None), 20);
        upsert_working_claude_subagent(&mut roster, "a", (&None, &None), 10);
        let snaps = claude_roster_to_snapshots(Some(&roster)).unwrap();
        assert_eq!(snaps[0].id, "a");
        assert_eq!(snaps[1].id, "b");
    }
}
