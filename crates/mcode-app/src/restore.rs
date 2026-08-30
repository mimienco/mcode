//! Startup restore planning from the persisted shutdown checkpoint.
//!
//! Pure planning only: reads the checkpoint snapshot that
//! [`mcode_settings::SettingsService::stage_shutdown_checkpoint`] committed at
//! quit time and decides what a fresh launch should reopen. The Electron
//! original reopened windows and rebound PTY sessions while honoring three
//! precedence rules, mirrored here through the settings crate's own
//! reconciliation: retirement tombstones are applied before bindings,
//! terminated or expired SSH leases are never resurrected, and stale terminal
//! topology is rebased to the host revision.

use mcode_platform::restorable_window_bounds;
use mcode_platform::window::{MonitorBounds, WindowBounds};
use mcode_settings::{ShutdownCheckpoint, apply_shutdown_checkpoint};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

/// Size bound for raw checkpoint bytes before parsing. Mirrors the settings
/// crate's 64 MiB persisted-state limit.
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;

/// Stable failure codes surfaced to the shell when a restore plan cannot be
/// derived from a readable-but-invalid checkpoint payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreError {
    /// The checkpoint payload is not valid checkpoint JSON or exceeds the
    /// persisted size bound.
    CheckpointUnreadable,
    /// The payload parses as JSON but violates the versioned checkpoint
    /// contract (unknown version, malformed envelope, invalid session shape).
    SchemaRejected,
}

impl RestoreError {
    pub fn code(self) -> &'static str {
        match self {
            Self::CheckpointUnreadable => "restore_checkpoint_unreadable",
            Self::SchemaRejected => "restore_schema_rejected",
        }
    }
}

impl fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CheckpointUnreadable => {
                "shutdown checkpoint payload is unreadable or exceeds the size bound"
            }
            Self::SchemaRejected => {
                "shutdown checkpoint payload violates the versioned checkpoint contract"
            }
        })
    }
}

impl std::error::Error for RestoreError {}

/// One terminal/agent session the renderer should rebind at startup.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorableSession {
    pub workspace_id: String,
    pub session_id: String,
    /// `local`, `ssh:<target>`, or `runtime:<id>` — the execution host the
    /// session ran on before shutdown.
    pub kind: String,
}

/// A window to reopen with its last-known bounds state. Bounds are absent
/// when the persisted placement was judged not restorable (near-minimum size
/// or no meaningful overlap with an attached monitor).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorableWindow {
    pub bounds: Option<WindowBounds>,
    pub maximized: bool,
}

/// What a fresh launch should restore, derived purely from the checkpoint.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlan {
    pub windows: Vec<RestorableWindow>,
    pub sessions: Vec<RestorableSession>,
    /// Sessions present in the checkpoint but excluded by retirement
    /// tombstones, fencing, or dead SSH leases — the renderer must not
    /// resurrect them.
    pub skipped: Vec<SkippedSession>,
    /// True when the checkpoint was stale or partially invalid: the plan is
    /// empty-but-valid and the shell should fall back to default placement
    /// and a clean workspace instead of failing the launch.
    pub incomplete: bool,
}

/// A session excluded from restore, with the reason it was excluded.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedSession {
    pub session_id: String,
    pub reason: SkipReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// A terminal-surface tombstone retired this pane before shutdown.
    Retired,
    /// The session was fenced off (removed workspace, pruned partition).
    Fenced,
    /// The SSH remote PTY lease backing this session terminated or expired.
    LeaseTerminated,
    /// The session record was structurally invalid and could not be read.
    Malformed,
}

/// Derives the startup restore plan from the persisted settings root and the
/// staged checkpoint.
///
/// `persisted` is the profile JSON as loaded by
/// [`mcode_settings::SettingsService`] (window placement under `ui`, SSH
/// remote PTY leases under `sshRemotePtyLeases`, per-host session partitions
/// under `workspaceSessionsByHostId`). The checkpoint is reconciled exactly
/// as the settings crate does at shutdown-commit time — retirement tombstones
/// before binding preservation, terminated/expired SSH leases suppressed,
/// stale topology rebased — and the reconciled result is diffed against the
/// checkpoint's own expectations to produce the plan. Never panics; returns
/// [`RestoreError`] when the checkpoint violates the versioned contract.
pub fn build_restore_plan(
    persisted: &Value,
    checkpoint: &ShutdownCheckpoint,
    monitors: &[MonitorBounds],
) -> Result<RestorePlan, RestoreError> {
    let reconciled = reconcile_checkpoint(persisted, checkpoint)?;
    let mut plan = RestorePlan {
        windows: vec![window_to_restore(reconciled.get("ui"), monitors)],
        ..RestorePlan::default()
    };

    let expected_state = |session: &mcode_settings::ShutdownSession| {
        serde_json::to_value(&session.state)
            .ok()
            .and_then(|value| value.as_object().cloned())
    };
    let expected_local = checkpoint
        .sessions
        .iter()
        .find(|session| session.host_id.is_none())
        .and_then(expected_state);
    if let Some(local) = reconciled
        .get("workspaceSession")
        .and_then(Value::as_object)
    {
        collect_restorable_sessions(local, expected_local.as_ref(), None, &mut plan);
    }
    let expected_hosts: std::collections::BTreeMap<String, Map<String, Value>> = checkpoint
        .sessions
        .iter()
        .filter_map(|session| {
            let host_id = serde_json::to_value(&session.host_id).ok()?;
            let host_id = host_id.as_str()?.to_owned();
            Some((host_id, expected_state(session)?))
        })
        .collect();
    if let Some(partitions) = reconciled
        .get("workspaceSessionsByHostId")
        .and_then(Value::as_object)
    {
        for (host_id, state) in partitions {
            if let Some(state) = state.as_object() {
                collect_restorable_sessions(
                    state,
                    expected_hosts.get(host_id),
                    Some(host_id),
                    &mut plan,
                );
            }
        }
    }
    plan.sessions.sort();
    plan.skipped.sort();
    Ok(plan)
}
/// Builds a plan from raw persisted-profile bytes plus raw checkpoint bytes.
///
/// Enforces the persisted-state size bound before parsing; any rejection —
/// unreadable bytes, schema-invalid envelope, unsupported version —
/// converts into an empty-but-valid plan with `incomplete: true` rather
/// than aborting the launch.
pub fn build_restore_plan_from_bytes(persisted: &[u8], checkpoint: &[u8]) -> RestorePlan {
    if persisted.len() > MAX_CHECKPOINT_BYTES || checkpoint.len() > MAX_CHECKPOINT_BYTES {
        return plan_from_error(RestoreError::CheckpointUnreadable);
    }
    let persisted: Value = match serde_json::from_slice(persisted) {
        Ok(value) => value,
        Err(_) => return plan_from_error(RestoreError::CheckpointUnreadable),
    };
    let checkpoint: ShutdownCheckpoint = match serde_json::from_slice(checkpoint) {
        Ok(checkpoint) => checkpoint,
        Err(_) => return plan_from_error(RestoreError::SchemaRejected),
    };
    match build_restore_plan(&persisted, &checkpoint, &[]) {
        Ok(plan) => plan,
        Err(_) => plan_from_error(RestoreError::SchemaRejected),
    }
}

/// Empty-but-valid fallback for a rejected checkpoint: restore nothing, mark
/// the plan incomplete so the shell falls back to defaults.
pub fn plan_from_error(error: RestoreError) -> RestorePlan {
    let _ = error;
    RestorePlan {
        incomplete: true,
        ..RestorePlan::default()
    }
}

/// Reconciles the checkpoint exactly as the settings crate does at
/// shutdown-commit time so the plan reflects the same tombstone-before-
/// bindings precedence, dead-lease suppression, and topology rebase.
fn reconcile_checkpoint(
    persisted: &Value,
    checkpoint: &ShutdownCheckpoint,
) -> Result<Value, RestoreError> {
    apply_shutdown_checkpoint(persisted.clone(), checkpoint.clone())
        .map_err(|_| RestoreError::SchemaRejected)
}

/// Projects the persisted UI object into the window entry. Bounds are read
/// leniently (`windowBounds`/`windowMaximized`) because the versioned
/// checkpoint's typed UI patch carries only `activeView`; placement fields
/// survive only when the persisted root already held them, and otherwise the
/// shell falls back to default placement through `bounds: None`.
fn window_to_restore(ui: Option<&Value>, monitors: &[MonitorBounds]) -> RestorableWindow {
    let fields = ui.and_then(Value::as_object);
    let bounds: Option<WindowBounds> = fields
        .and_then(|fields| fields.get("windowBounds"))
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .and_then(|bounds| restorable_window_bounds(Some(bounds), monitors));
    let maximized = fields
        .and_then(|fields| fields.get("windowMaximized"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && bounds.is_some();
    RestorableWindow { bounds, maximized }
}

/// Projects one reconciled session partition into plan entries.
///
/// `expected` is the same partition as the checkpoint carried it, before
/// reconciliation cleared the tombstone map and suppressed dead bindings:
/// reconciliation is authoritative, so a pane retired by a tombstone (the
/// tombstone map is consumed and emptied) or whose leaf binding was dropped
/// (a terminated or expired SSH lease matched) is reported as skipped, and
/// only surviving leaf bindings are projected as restorable.
fn collect_restorable_sessions(
    reconciled: &Map<String, Value>,
    expected: Option<&Map<String, Value>>,
    host_id: Option<&str>,
    plan: &mut RestorePlan,
) {
    // Panes the checkpoint expected but reconciliation removed are either
    // tombstone-retired or fenced (workspace removal); surviving-but-dead
    // leases are reported with the lease reason below.
    if let Some(expected) = expected {
        let mut tombstones = record(expected, "terminalSurfaceTombstonesByPaneKey");
        for pane_key in record(reconciled, "terminalSurfaceTombstonesByPaneKey").keys() {
            tombstones.insert(pane_key.clone(), Value::Null);
        }
        let mut retired: BTreeMap<String, String> = BTreeMap::new();
        for pane_key in tombstones.keys() {
            retired.insert(pane_key.clone(), session_identity(expected, pane_key));
        }
        for session_id in retired.into_values() {
            plan.skipped.push(SkippedSession {
                session_id,
                reason: SkipReason::Retired,
            });
        }
    }

    let dead_lease_ptys = dead_lease_ptys(reconciled);
    let layouts = record(reconciled, "terminalLayoutsByTabId");
    let tabs_by_worktree = record(reconciled, "tabsByWorktree");
    let mut sessions: BTreeMap<(String, String, String), ()> = BTreeMap::new();
    for (tab_id, layout) in &layouts {
        let Some(layout) = layout.as_object() else {
            continue;
        };
        let Some(bindings) = layout.get("ptyIdsByLeafId").and_then(Value::as_object) else {
            continue;
        };
        for (leaf_id, pty) in bindings {
            let Some(pty_id) = pty.as_str() else {
                continue;
            };
            let pane_key = format!("{tab_id}:{leaf_id}");
            let session_id = session_identity(reconciled, &pane_key);
            if dead_lease_ptys.contains_key(pty_id) {
                plan.skipped.push(SkippedSession {
                    session_id,
                    reason: SkipReason::LeaseTerminated,
                });
                continue;
            }
            let workspace_id = worktree_for_tab(&tabs_by_worktree, tab_id);
            let kind = host_id.unwrap_or("local").to_owned();
            sessions.insert((workspace_id, session_id, kind), ());
        }
    }
    for ((workspace_id, session_id, kind), ()) in sessions {
        plan.sessions.push(RestorableSession {
            workspace_id,
            session_id,
            kind,
        });
    }
}

/// Collects the comparable PTY ids of terminated or expired SSH remote
/// leases. Binding ids are SSH relay forms (`ssh:<target>@@<relay>`), so the
/// lease id and the bound id only match after the same normalization the
/// settings crate applies when suppressing restorable bindings.
fn dead_lease_ptys(state: &Map<String, Value>) -> BTreeMap<String, ()> {
    let mut dead = BTreeMap::new();
    for lease in state
        .get("sshRemotePtyLeases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(lease) = lease.as_object() else {
            continue;
        };
        let Some(state) = lease.get("state").and_then(Value::as_str) else {
            continue;
        };
        if !matches!(state, "terminated" | "expired") {
            continue;
        }
        let Some(lease_pty_id) = lease.get("ptyId").and_then(Value::as_str) else {
            continue;
        };
        dead.insert(lease_pty_id.to_owned(), ());
        if let Some(target_id) = lease.get("targetId").and_then(Value::as_str) {
            dead.insert(ssh_relay_pty_id(target_id, lease_pty_id), ());
        }
    }
    dead
}

/// Rebuilds the SSH relay binding id (`ssh:<encoded-target>@@<relay>`) a
/// renderer would have written for this target and relay id, matching the
/// settings crate's `comparable_pty_id` normalization in reverse.
fn ssh_relay_pty_id(target_id: &str, relay_id: &str) -> String {
    format!("ssh:{target_id}@@{relay_id}")
}

/// Deterministic session identity for plan projection: the pane's sleeping
/// agent session identity when present, else the pane key itself.
fn session_identity(state: &Map<String, Value>, pane_key: &str) -> String {
    state
        .get("sleepingAgentSessionsByPaneKey")
        .and_then(Value::as_object)
        .and_then(|sessions| sessions.get(pane_key))
        .and_then(Value::as_object)
        .and_then(|session| session.get("paneKey"))
        .and_then(Value::as_str)
        .unwrap_or(pane_key)
        .to_owned()
}

fn worktree_for_tab(tabs_by_worktree: &Map<String, Value>, tab_id: &str) -> String {
    for (worktree_id, tabs) in tabs_by_worktree {
        let Some(tabs) = tabs.as_array() else {
            continue;
        };
        for tab in tabs {
            let Some(tab) = tab.as_object() else {
                continue;
            };
            if tab.get("id").and_then(Value::as_str) == Some(tab_id) {
                return worktree_id.clone();
            }
        }
    }
    String::new()
}

fn record(state: &Map<String, Value>, field: &str) -> Map<String, Value> {
    state
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}
