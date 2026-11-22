//! Pane-keyed status store — Rust port of `AgentHookServer.applyNormalizedStatus`
//! (`server.ts:1321-1507`), the snapshot projection (`:438-454`), drop/clear
//! paths (`:2578-2802`), the disposition gate (`:1129-1158`), pane-key alias
//! machinery (`:1651-1998`), and the migration-unsupported registry.

use std::collections::HashMap;

use crate::listener_state::{HookListenerState, clear_pane_cache_state};
use crate::types::{
    AgentHookEventPayload, AgentStatusClearIpcPayload, AgentStatusIpcPayload, AgentStatusState,
    EnrichedAgentHookEventPayload, MigrationUnsupportedPtyEntry,
};

/// Effects the tauri layer maps onto renderer events.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)] // pinned D-013 signature; Set is the full IPC payload
pub enum StoreEffect {
    Set(AgentStatusIpcPayload),
    Clear(AgentStatusClearIpcPayload),
    MigrationUnsupported(MigrationUnsupportedPtyEntry),
    MigrationUnsupportedClear { pty_id: String },
}

#[derive(Debug)]
pub struct StatusStore {
    state: HookListenerState,
    entries: HashMap<String, EnrichedAgentHookEventPayload>,
    closed_tab_ids: Vec<String>,
    closed_pane_keys: Vec<String>,
    /// paneKey → {stablePaneKey} alias map (insertion-ordered, bounded 1024).
    legacy_pane_key_aliases: Vec<(String, String)>,
    migration_unsupported: HashMap<String, MigrationUnsupportedPtyEntry>,
    persist_dir: Option<std::path::PathBuf>,
    dirty: bool,
    hydrated_token_hashes: HashMap<String, String>,
    authority_commitments: serde_json::Map<String, serde_json::Value>,
    retired_fences: Vec<RetiredPaneFence>,
}

#[derive(Debug)]
struct RetiredPaneFence {
    pane_keys: Vec<String>,
    aliases: Vec<(String, String)>,
}

/// Shared handle the tauri layer holds in a OnceLock.
pub type StoreHandle = std::sync::Arc<std::sync::Mutex<StatusStore>>;

impl StatusStore {
    /// `None` = tests, no disk. `Some(dir)` = hydrate last-status.json v2 and
    /// debounced-persist on apply.
    pub fn new(persist_dir: Option<std::path::PathBuf>) -> Self {
        let mut store = Self {
            state: HookListenerState::default(),
            entries: HashMap::new(),
            closed_tab_ids: Vec::new(),
            closed_pane_keys: Vec::new(),
            legacy_pane_key_aliases: Vec::new(),
            migration_unsupported: HashMap::new(),
            persist_dir,
            dirty: false,
            hydrated_token_hashes: HashMap::new(),
            authority_commitments: serde_json::Map::new(),
            retired_fences: Vec::new(),
        };
        store.hydrate();
        store
    }

    /// Late-attach the persist dir (the shared store is constructed before
    /// the app handle resolves userData) and re-run hydration.
    pub fn attach_persist_dir(&mut self, persist_dir: std::path::PathBuf) {
        self.persist_dir = Some(persist_dir);
        self.hydrate();
    }

    /// Re-run hydration against the current persist dir (idempotent clears).
    pub fn rehydrate(&mut self) {
        self.hydrate();
    }

    pub fn state(&self) -> &HookListenerState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut HookListenerState {
        &mut self.state
    }

    pub fn entries(&self) -> &std::collections::HashMap<String, EnrichedAgentHookEventPayload> {
        &self.entries
    }

    pub fn entries_mut(
        &mut self,
    ) -> &mut std::collections::HashMap<String, EnrichedAgentHookEventPayload> {
        &mut self.entries
    }

    /// Split borrow for inference entry points (state + rows in one call).
    pub fn state_and_entries_mut(
        &mut self,
    ) -> (
        &mut HookListenerState,
        &mut std::collections::HashMap<String, EnrichedAgentHookEventPayload>,
    ) {
        (&mut self.state, &mut self.entries)
    }

    pub fn infer_interrupt(
        &mut self,
        request: &crate::types::AgentInterruptInferenceRequest,
        now: i64,
    ) -> bool {
        let applied =
            crate::infer::infer_interrupt(&mut self.state, &mut self.entries, request, now);
        self.dirty |= applied;
        applied
    }

    pub fn infer_question_answered(
        &mut self,
        request: &crate::types::AgentQuestionAnsweredInferenceRequest,
        now: i64,
    ) -> bool {
        let applied =
            crate::infer::infer_question_answered(&mut self.state, &mut self.entries, request, now);
        self.dirty |= applied;
        applied
    }

    pub fn accepts_pane(&self, pane_key: &str) -> bool {
        crate::types::is_valid_pane_key(&self.resolve_alias(pane_key)) && self.disposition(pane_key)
    }

    pub fn resolve_pane_key(&self, pane_key: &str) -> String {
        self.resolve_alias(pane_key)
    }

    pub fn can_transfer_pane_authority(
        &self,
        from: &str,
        pty_id: Option<&str>,
        owns_pty: &dyn Fn(&str, &str) -> bool,
    ) -> bool {
        if !crate::types::is_valid_pane_key(from) || !self.disposition(from) {
            return false;
        }
        let owner = self.resolve_alias(from);
        match pty_id.filter(|id| !id.trim().is_empty()) {
            Some(id) => owns_pty(from, id) || (owner != from && owns_pty(&owner, id)),
            None => self
                .legacy_pane_key_aliases
                .iter()
                .any(|(alias, _)| alias == from),
        }
    }

    /// `retirePaneAuthority` (`server.ts:1851-1892`) — local-host shape:
    /// fence the pane and its aliases so a re-attach cannot resurrect rows.
    pub fn retire_pane_authority(&mut self, pane_key: &str) {
        let owner = self.resolve_alias(pane_key);
        let aliases: Vec<_> = self
            .legacy_pane_key_aliases
            .iter()
            .filter(|(alias, target)| alias == pane_key || target == &owner)
            .cloned()
            .collect();
        let mut keys = vec![pane_key.to_string(), owner.clone()];
        keys.extend(
            aliases
                .iter()
                .flat_map(|(from, to)| [from.clone(), to.clone()]),
        );
        keys.sort();
        keys.dedup();
        for key in &keys {
            self.mark_pane_closed(key);
            self.dirty |= self.entries.remove(key).is_some();
            self.state.last_status_by_pane_key.remove(key);
            clear_pane_cache_state(&mut self.state, key);
            self.dirty |= self.authority_commitments.remove(key).is_some();
            self.hydrated_token_hashes.remove(key);
        }
        self.legacy_pane_key_aliases
            .retain(|(alias, _)| !aliases.iter().any(|(old, _)| old == alias));
        self.retired_fences
            .retain(|fence| !fence.pane_keys.iter().any(|key| keys.contains(key)));
        self.retired_fences.push(RetiredPaneFence {
            pane_keys: keys,
            aliases,
        });
        while self.retired_fences.len() > crate::types::RETIRED_PANE_FENCES_MAX {
            self.retired_fences.remove(0);
        }
    }

    /// `restorePaneAuthority` (`server.ts:1924-1946`) — closed tab wins.
    pub fn restore_pane_authority(&mut self, pane_key: &str) -> bool {
        let owner = self.resolve_alias(pane_key);
        if self.is_closed_tab_for_pane_key(&owner) || self.is_closed_tab_for_pane_key(pane_key) {
            return false;
        }
        let mut keys = vec![pane_key.to_string(), owner];
        if let Some(index) = self
            .retired_fences
            .iter()
            .position(|fence| fence.pane_keys.iter().any(|key| keys.contains(key)))
        {
            let fence = self.retired_fences.remove(index);
            for (alias, target) in fence.aliases {
                if !self.is_closed_tab_for_pane_key(&alias)
                    && !self.is_closed_tab_for_pane_key(&target)
                    && !self
                        .legacy_pane_key_aliases
                        .iter()
                        .any(|(existing, _)| existing == &alias)
                {
                    self.legacy_pane_key_aliases.push((alias, target));
                }
            }
            keys.extend(fence.pane_keys);
        }
        let keys: Vec<_> = keys
            .into_iter()
            .filter(|key| !self.is_closed_tab_for_pane_key(key))
            .collect();
        let had = self.closed_pane_keys.iter().any(|key| keys.contains(key));
        self.closed_pane_keys.retain(|key| !keys.contains(key));
        had
    }

    /// `canTransferPaneAuthority` + `transferPaneAuthority`
    /// (`server.ts:1693-1856`), local-host shape: the ptyId authority check is
    /// renderer-supplied under Tauri (the PTY map lives there).
    pub fn transfer_pane_authority(&mut self, from: &str, to: &str, pty_id: Option<&str>) -> bool {
        if from == to
            || !crate::types::is_valid_pane_key(from)
            || !crate::types::is_valid_pane_key(to)
            || !self.disposition(to)
        {
            return false;
        }
        let owner = self.resolve_alias(from);
        if owner != to && self.entries.contains_key(to) {
            return false;
        }
        // Without a ptyId only an existing alias path may transfer.
        if pty_id.is_none() && !self.legacy_pane_key_aliases.iter().any(|(l, _)| l == from) {
            return false;
        }
        // Rebind the status row + caches from the owner to the target
        // (movePaneCacheState + row rewrite, server.ts:1767-1856). Wave-37
        // scope: the renderer never transfers panes yet; the plumbing is the
        // contract.
        if let Some(row) = self.entries.remove(&owner) {
            self.entries.insert(
                to.to_string(),
                EnrichedAgentHookEventPayload {
                    event: AgentHookEventPayload {
                        pane_key: to.to_string(),
                        tab_id: crate::types::parse_pane_key(to).map(|(tab, _)| tab),
                        ..row.event
                    },
                    ..row
                },
            );
            self.dirty = true;
        }
        if let Some(event) = self.state.last_status_by_pane_key.remove(&owner) {
            self.state.last_status_by_pane_key.insert(
                to.to_string(),
                AgentHookEventPayload {
                    pane_key: to.to_string(),
                    tab_id: crate::types::parse_pane_key(to).map(|(tab, _)| tab),
                    ..event
                },
            );
        }
        if owner != to {
            move_listener_pane(&mut self.state, &owner, to);
            for (_, target) in &mut self.legacy_pane_key_aliases {
                if target == &owner {
                    *target = to.to_string();
                }
            }
            self.legacy_pane_key_aliases
                .retain(|(alias, _)| alias != from);
            self.legacy_pane_key_aliases
                .push((from.to_string(), to.to_string()));
            while self.legacy_pane_key_aliases.len() > crate::types::PANE_KEY_ALIASES_MAX {
                self.legacy_pane_key_aliases.remove(0);
            }
            if let Some(hash) = self.hydrated_token_hashes.remove(&owner) {
                self.hydrated_token_hashes.insert(to.to_string(), hash);
            }
            if let Some(mut commitment) = self.authority_commitments.remove(&owner) {
                commitment["paneKey"] = serde_json::json!(to);
                if let Some((tab, _)) = crate::types::parse_pane_key(to) {
                    commitment["tabId"] = serde_json::json!(tab);
                }
                self.authority_commitments
                    .insert(to.to_string(), commitment);
                self.dirty = true;
            }
        }
        true
    }

    // ── disposition gate (`server.ts:1129-1158`) ────────────────────────────

    fn mark_tab_closed(&mut self, tab_id: &str) {
        // Delete-then-add keeps recently closed tabs most-recent.
        self.closed_tab_ids.retain(|id| id != tab_id);
        self.closed_tab_ids.push(tab_id.to_string());
        while self.closed_tab_ids.len() > crate::types::CLOSED_AGENT_STATUS_TAB_IDS_MAX {
            self.closed_tab_ids.remove(0);
        }
    }

    fn mark_pane_closed(&mut self, pane_key: &str) {
        self.closed_pane_keys.retain(|key| key != pane_key);
        self.closed_pane_keys.push(pane_key.to_string());
        while self.closed_pane_keys.len() > crate::types::CLOSED_AGENT_STATUS_PANE_KEYS_MAX {
            self.closed_pane_keys.remove(0);
        }
    }

    fn is_closed_tab_for_pane_key(&self, pane_key: &str) -> bool {
        let tab_id = crate::types::parse_pane_key(pane_key)
            .map(|(tab, _)| tab)
            .or_else(|| crate::types::parse_legacy_numeric_pane_key(pane_key).map(|(tab, _)| tab));
        tab_id.is_some_and(|tab| self.closed_tab_ids.contains(&tab))
    }

    fn resolve_alias(&self, pane_key: &str) -> String {
        self.legacy_pane_key_aliases
            .iter()
            .find(|(legacy, _)| legacy == pane_key)
            .map(|(_, stable)| stable.clone())
            .unwrap_or_else(|| pane_key.to_string())
    }

    /// 'accept' | 'suppress' — the 'restart' arm (retired-pane lift) is a
    /// wave-38 relay concern; local HTTP ingest never re-opens a retired pane.
    fn disposition(&self, pane_key: &str) -> bool {
        let owner = self.resolve_alias(pane_key);
        !(self.closed_pane_keys.iter().any(|key| key == pane_key)
            || self.closed_pane_keys.contains(&owner)
            || self.is_closed_tab_for_pane_key(pane_key)
            || self.is_closed_tab_for_pane_key(&owner))
    }

    // ── apply (`applyNormalizedStatus` port) ────────────────────────────────

    pub fn apply(&mut self, mut event: AgentHookEventPayload, now: i64) -> Vec<StoreEffect> {
        if !self.disposition(&event.pane_key) {
            // Closed tab/pane: suppress silently (getAgentStatusDisposition).
            return Vec::new();
        }
        let resolved = self.resolve_alias(&event.pane_key);
        if resolved != event.pane_key {
            event.tab_id = crate::types::parse_pane_key(&resolved).map(|(tab, _)| tab);
        }
        event.pane_key = resolved;
        apply_enriched(&mut self.state, &mut self.entries, event, now).map_or_else(
            Vec::new,
            |payload| {
                self.dirty = true;
                vec![StoreEffect::Set(payload)]
            },
        )
    }

    /// Snapshot projection (`toAgentStatusIpcPayload`, `server.ts:438-454`).
    pub fn snapshot_ipc(&self) -> Vec<AgentStatusIpcPayload> {
        let mut rows: Vec<&EnrichedAgentHookEventPayload> = self.entries.values().collect();
        rows.sort_by_key(|a| a.received_at);
        rows.iter().map(|entry| to_ipc_payload(entry)).collect()
    }

    // ── drop/clear (`server.ts:2578-2600`, `:2650-2675`) ────────────────────

    /// User dismissal — drop only the status row; do NOT wipe prompt/tool
    /// caches since the pane's agent may still be alive.
    pub fn drop_status_entry(&mut self, pane_key: &str, _now: i64) -> Vec<StoreEffect> {
        let resolved = self.resolve_alias(pane_key);
        let Some(existing) = self.entries.remove(&resolved) else {
            return Vec::new();
        };
        self.state.last_status_by_pane_key.remove(&resolved);
        // preserveAuthority: true — dismissal is not PTY teardown.
        self.dirty = true;
        let effects = vec![StoreEffect::Clear(AgentStatusClearIpcPayload::Pane {
            pane_key: resolved.clone(),
        })];
        if existing.event.provider_session.is_some()
            && existing.event.payload.agent_type.is_some()
            && existing.event.payload.agent_type.as_deref() != Some("unknown")
        {
            // Retained for liveness: keep the resume identity, drop the UI row.
            let retained = EnrichedAgentHookEventPayload {
                event: AgentHookEventPayload {
                    provider_session_only: true,
                    ..existing.event.clone()
                },
                retained_for_liveness: true,
                claude_lead_boundary_child_only: existing.claude_lead_boundary_child_only,
                received_at: existing.received_at,
                state_started_at: existing.state_started_at,
                observation: None,
                restored_unconfirmed: false,
            };
            self.entries.insert(resolved.clone(), retained);
            self.state.last_status_by_pane_key.insert(
                resolved.clone(),
                crate::types::AgentHookEventPayload {
                    provider_session_only: true,
                    ..existing.event.clone()
                },
            );
        }
        effects
    }

    /// Explicit tab close — drop all rows under the tab prefix.
    pub fn drop_status_entries_by_tab_prefix(&mut self, tab_id: &str) -> Vec<StoreEffect> {
        self.mark_tab_closed(tab_id);
        self.hydrated_token_hashes.retain(|key, _| {
            crate::types::parse_pane_key(key).is_none_or(|(tab, _)| tab != tab_id)
        });
        let commitments_before = self.authority_commitments.len();
        self.authority_commitments.retain(|key, _| {
            crate::types::parse_pane_key(key).is_none_or(|(tab, _)| tab != tab_id)
        });
        self.dirty |= commitments_before != self.authority_commitments.len();
        let pane_keys_to_clear: Vec<String> = self
            .entries
            .keys()
            .filter(|key| {
                crate::types::parse_pane_key(key)
                    .map(|(key_tab, _)| key_tab == tab_id)
                    .or_else(|| {
                        crate::types::parse_legacy_numeric_pane_key(key)
                            .map(|(key_tab, _)| key_tab == tab_id)
                    })
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        let mut effects = Vec::new();
        for pane_key in pane_keys_to_clear {
            let resolved = self.resolve_alias(&pane_key);
            if self.entries.remove(&resolved).is_some() {
                self.state.last_status_by_pane_key.remove(&resolved);
                effects.push(StoreEffect::Clear(AgentStatusClearIpcPayload::Pane {
                    pane_key: resolved.clone(),
                }));
            }
            clear_pane_cache_state(&mut self.state, &resolved);
            self.mark_pane_closed(&resolved);
            let alias_keys: Vec<String> = self
                .legacy_pane_key_aliases
                .iter()
                .filter(|(_, stable)| stable == &resolved)
                .map(|(legacy, _)| legacy.clone())
                .collect();
            for legacy in alias_keys {
                self.legacy_pane_key_aliases.retain(|(l, _)| l != &legacy);
                clear_pane_cache_state(&mut self.state, &legacy);
                self.mark_pane_closed(&legacy);
            }
            clear_migration_unsupported_for_pane(
                &mut self.migration_unsupported,
                &resolved,
                &mut effects,
            );
        }
        effects.extend(self.clear_migration_unsupported_by_tab_prefix(tab_id));
        if !effects.is_empty() {
            self.dirty = true;
        }
        effects
    }

    /// PTY teardown path — clears caches too and emits the clear event.
    pub fn clear_pane_state(&mut self, pane_key: &str) -> Vec<StoreEffect> {
        let resolved = self.resolve_alias(pane_key);
        let had_status = self.entries.contains_key(&resolved);
        clear_pane_cache_state(&mut self.state, &resolved);
        self.hydrated_token_hashes.remove(&resolved);
        self.dirty |= self.authority_commitments.remove(&resolved).is_some();
        let existed = self.entries.remove(&resolved).is_some();
        self.state.last_status_by_pane_key.remove(&resolved);
        let mut effects = Vec::new();
        let alias_keys: Vec<String> = self
            .legacy_pane_key_aliases
            .iter()
            .filter(|(_, stable)| stable == &resolved)
            .map(|(legacy, _)| legacy.clone())
            .collect();
        for legacy in alias_keys {
            self.legacy_pane_key_aliases.retain(|(l, _)| l != &legacy);
            clear_pane_cache_state(&mut self.state, &legacy);
            self.state.last_status_by_pane_key.remove(&legacy);
        }
        if had_status || existed {
            self.dirty = true;
            effects.push(StoreEffect::Clear(AgentStatusClearIpcPayload::Pane {
                pane_key: resolved.clone(),
            }));
        }
        effects
    }

    pub fn set_migration_unsupported(
        &mut self,
        entry: MigrationUnsupportedPtyEntry,
    ) -> Vec<StoreEffect> {
        self.migration_unsupported
            .insert(entry.pty_id.clone(), entry.clone());
        vec![StoreEffect::MigrationUnsupported(entry)]
    }

    pub fn clear_migration_unsupported(&mut self, pty_id: &str) -> Vec<StoreEffect> {
        if self.migration_unsupported.remove(pty_id).is_some() {
            vec![StoreEffect::MigrationUnsupportedClear {
                pty_id: pty_id.to_string(),
            }]
        } else {
            Vec::new()
        }
    }

    pub fn clear_migration_unsupported_for_pane_key(&mut self, pane_key: &str) -> Vec<StoreEffect> {
        let mut effects = Vec::new();
        clear_migration_unsupported_for_pane(
            &mut self.migration_unsupported,
            pane_key,
            &mut effects,
        );
        effects
    }

    pub fn clear_migration_unsupported_by_tab_prefix(&mut self, tab_id: &str) -> Vec<StoreEffect> {
        let mut effects = Vec::new();
        let prefix = format!("{tab_id}:");
        let matched: Vec<String> = self
            .migration_unsupported
            .values()
            .filter(|entry| {
                entry
                    .pane_key
                    .as_deref()
                    .is_some_and(|key| key.starts_with(&prefix))
            })
            .map(|entry| entry.pty_id.clone())
            .collect();
        for pty_id in matched {
            if self.migration_unsupported.remove(&pty_id).is_some() {
                effects.push(StoreEffect::MigrationUnsupportedClear { pty_id });
            }
        }
        effects
    }

    pub fn migration_unsupported_snapshot(&self) -> Vec<MigrationUnsupportedPtyEntry> {
        let mut rows: Vec<MigrationUnsupportedPtyEntry> =
            self.migration_unsupported.values().cloned().collect();
        rows.sort_by_key(|a| a.updated_at);
        rows
    }

    // ── persistence (`server.ts:3111-3237` + `:2923-3063`) ──────────────────

    fn hydrate(&mut self) {
        let Some(dir) = &self.persist_dir else { return };
        self.entries.clear();
        self.state.last_status_by_pane_key.clear();
        crate::listener_state::clear_all_listener_caches(&mut self.state);
        self.hydrated_token_hashes.clear();
        self.authority_commitments.clear();
        let Ok(raw) = std::fs::read_to_string(dir.join("last-status.json")) else {
            return;
        };
        let Ok(file) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return;
        };
        if file.get("version").and_then(serde_json::Value::as_i64)
            != Some(crate::types::LAST_STATUS_FILE_VERSION as i64)
        {
            return; // fail closed on version mismatch
        }
        let Some(entries) = file.get("entries").and_then(serde_json::Value::as_object) else {
            return;
        };
        let now = epoch_now();
        let ttl_cutoff = now - crate::types::HYDRATE_MAX_AGE_MS;
        if let Some(commitments) = file
            .get("authorityCommitments")
            .and_then(serde_json::Value::as_object)
        {
            for (pane_key, commitment) in commitments {
                if crate::types::is_valid_pane_key(pane_key)
                    && commitment
                        .get("paneKey")
                        .and_then(serde_json::Value::as_str)
                        == Some(pane_key.as_str())
                    && commitment
                        .get("launchTokenHash")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(valid_token_hash)
                    && commitment
                        .get("observedAt")
                        .and_then(serde_json::Value::as_i64)
                        .is_some_and(|time| time >= ttl_cutoff && time <= now)
                {
                    self.authority_commitments
                        .insert(pane_key.clone(), commitment.clone());
                }
            }
        }
        for (pane_key, raw_entry) in entries {
            if !crate::types::is_valid_pane_key(pane_key) {
                continue;
            }
            let Ok(mut event) = serde_json::from_value::<AgentHookEventPayload>(raw_entry.clone())
            else {
                continue;
            };
            let received_at = raw_entry
                .get("receivedAt")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            if received_at <= 0
                || received_at < ttl_cutoff
                || event.pane_key != *pane_key
                || event.tab_id.as_deref().is_some_and(|tab| {
                    crate::types::parse_pane_key(pane_key)
                        .is_none_or(|(expected, _)| tab != expected)
                })
                || event.worktree_id.as_deref().is_some_and(str::is_empty)
            {
                continue;
            }
            let state_started_at = raw_entry
                .get("stateStartedAt")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            if state_started_at <= 0 {
                continue;
            }
            let claude_lead_boundary_child_only = raw_entry
                .get("claudeLeadBoundaryChildOnly")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            let retained_for_liveness = raw_entry
                .get("retainedForLiveness")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            if event.payload.agent_type.as_deref() == Some("claude")
                && let Some(subagents) = &mut event.payload.subagents
            {
                let before = subagents.len();
                subagents.retain(|agent| agent.state != crate::types::AgentSubagentState::Idle);
                self.dirty |= subagents.len() != before;
                if subagents.is_empty() {
                    event.payload.subagents = None;
                }
            }
            // Non-done ⇒ restoredUnconfirmed: a row that ended while no
            // receiver was up must not claim liveness.
            let restored_unconfirmed = event.payload.state != AgentStatusState::Done;
            let legacy_token = event
                .launch_token
                .take()
                .filter(|token| !token.trim().is_empty());
            if legacy_token.is_some() {
                self.dirty = true;
            }
            event.prompt_interaction_key = None;
            event.restored_unconfirmed = false;
            event.claude_running_non_agent_task = false;
            if let Some(hash) = raw_entry
                .get("launchTokenHash")
                .and_then(serde_json::Value::as_str)
                .filter(|hash| valid_token_hash(hash))
            {
                self.hydrated_token_hashes
                    .insert(pane_key.clone(), hash.to_string());
            } else if let Some(token) = legacy_token {
                use sha2::{Digest, Sha256};
                self.hydrated_token_hashes.insert(
                    pane_key.clone(),
                    format!("{:x}", Sha256::digest(token.trim().as_bytes())),
                );
            }
            let resolved = self.resolve_alias(pane_key);
            if event.payload.agent_type.as_deref() == Some("claude") {
                crate::normalizers::claude::seed_claude_lead_turn_from_persisted_status(
                    &mut self.state,
                    &resolved,
                    &event.payload,
                    claude_lead_boundary_child_only,
                );
                if let Some(subagents) = &event.payload.subagents {
                    crate::normalizers::claude_roster::seed_claude_subagent_roster_from_snapshots(
                        &mut self.state.claude_subagent_rosters,
                        &resolved,
                        subagents,
                    );
                }
            }
            self.state
                .last_prompt_by_pane_key
                .insert(resolved.clone(), event.payload.prompt.clone());
            self.state
                .last_status_by_pane_key
                .insert(resolved.clone(), event.clone());
            self.entries.insert(
                resolved.clone(),
                EnrichedAgentHookEventPayload {
                    event,
                    claude_lead_boundary_child_only,
                    retained_for_liveness,
                    received_at,
                    state_started_at,
                    observation: None,
                    restored_unconfirmed,
                },
            );
        }
    }

    /// Serialize v2 (`serializeStatusFile`): strip transport-only fields, skip
    /// invalid pane keys, atomic tmp+rename. Call on a debounce thread.
    pub fn persist_if_dirty(&mut self) -> std::io::Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        let Some(dir) = &self.persist_dir else {
            return Ok(false);
        };
        use sha2::{Digest, Sha256};
        let mut entries = serde_json::Map::new();
        let mut commitments = self.authority_commitments.clone();
        for (pane_key, entry) in &self.entries {
            if !crate::types::is_valid_pane_key(pane_key) {
                continue;
            }
            let mut value = serde_json::to_value(&entry.event)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
            if let Some(obj) = value.as_object_mut() {
                // launchToken / claudeRunningNonAgentTask / restoredUnconfirmed
                // are transport/runtime fields and must not survive disk.
                if entry.claude_lead_boundary_child_only {
                    obj.insert(
                        "claudeLeadBoundaryChildOnly".into(),
                        serde_json::json!(true),
                    );
                }
                if entry.retained_for_liveness {
                    obj.insert("retainedForLiveness".into(), serde_json::json!(true));
                }
                obj.remove("launchToken");
                obj.remove("claudeRunningNonAgentTask");
                obj.remove("restoredUnconfirmed");
                obj.remove("promptInteractionKey");
                let hash = entry
                    .event
                    .launch_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|token| !token.is_empty())
                    .map(|token| format!("{:x}", Sha256::digest(token.as_bytes())))
                    .or_else(|| self.hydrated_token_hashes.get(pane_key).cloned());
                if let Some(hash) = hash {
                    obj.insert("launchTokenHash".into(), serde_json::json!(hash));
                    let mut commitment = serde_json::json!({"paneKey": pane_key, "launchTokenHash": hash, "connectionId": entry.event.connection_id, "observedAt": entry.received_at});
                    if let Some(tab) = &entry.event.tab_id {
                        commitment["tabId"] = serde_json::json!(tab);
                    }
                    if let Some(worktree) = &entry.event.worktree_id {
                        commitment["worktreeId"] = serde_json::json!(worktree);
                    }
                    let conflicts = commitments.get(pane_key).is_some_and(|existing| {
                        [
                            "paneKey",
                            "launchTokenHash",
                            "connectionId",
                            "tabId",
                            "worktreeId",
                        ]
                        .iter()
                        .any(|field| existing.get(*field) != commitment.get(*field))
                    });
                    if conflicts {
                        commitments.remove(pane_key);
                    } else {
                        commitments.insert(pane_key.clone(), commitment);
                    }
                }
                obj.insert("receivedAt".into(), serde_json::json!(entry.received_at));
                obj.insert(
                    "stateStartedAt".into(),
                    serde_json::json!(entry.state_started_at),
                );
            }
            entries.insert(pane_key.clone(), value);
        }
        let file = serde_json::json!({
            "version": crate::types::LAST_STATUS_FILE_VERSION,
            "entries": entries,
            "authorityCommitments": commitments,
        });
        std::fs::create_dir_all(dir)?;
        use std::io::Write;
        let tmp = dir.join(format!(
            ".last-status-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(&tmp)?;
        let bytes = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
        let written = output.write_all(&bytes).and_then(|_| output.sync_all());
        drop(output);
        let result = written.and_then(|_| std::fs::rename(&tmp, dir.join("last-status.json")));
        if let Err(error) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
        self.dirty = false;
        Ok(true)
    }
}

fn valid_token_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_tool_progress_after_interrupt(event: &AgentHookEventPayload) -> bool {
    matches!(
        event.payload.agent_type.as_deref(),
        Some("claude" | "codex")
    ) && matches!(
        event.hook_event_name.as_deref(),
        Some("PreToolUse" | "PostToolUse" | "PostToolUseFailure")
    )
}

fn child_only_boundary(
    previous: Option<&EnrichedAgentHookEventPayload>,
    next: &AgentHookEventPayload,
) -> bool {
    if next.payload.agent_type.as_deref() != Some("claude") || next.claude_running_non_agent_task {
        return false;
    }
    let establishes = matches!(
        next.hook_event_name.as_deref(),
        Some("Stop" | "StopFailure")
    ) && next.tool_agent_id.is_none()
        && next.payload.state == AgentStatusState::Working
        && next.payload.subagents.as_ref().is_some_and(|agents| {
            agents
                .iter()
                .any(|agent| agent.state == crate::types::AgentSubagentState::Working)
        });
    let carries = previous.is_some_and(|old| old.claude_lead_boundary_child_only)
        && (next.tool_agent_id.is_some()
            || matches!(
                next.hook_event_name.as_deref(),
                Some("SubagentStart" | "SubagentStop" | "TeammateIdle")
            ));
    establishes || carries
}

fn inherit_permission_tool_id(
    previous: &EnrichedAgentHookEventPayload,
    next: &mut AgentHookEventPayload,
) {
    let old = &previous.event;
    if !previous.restored_unconfirmed
        && old.payload.agent_type.as_deref() == Some("claude")
        && old.payload.state == AgentStatusState::Working
        && old.hook_event_name.as_deref() == Some("PreToolUse")
        && old
            .tool_use_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty())
        && next.payload.agent_type.as_deref() == Some("claude")
        && next.payload.state == AgentStatusState::Waiting
        && next.hook_event_name.as_deref() == Some("PermissionRequest")
        && next.tool_use_id.is_none()
        && old.tool_agent_id == next.tool_agent_id
        && old.tool_agent_type == next.tool_agent_type
        && old.payload.tool_name.is_some()
        && old.payload.tool_name == next.payload.tool_name
        && old.payload.tool_input == next.payload.tool_input
    {
        next.tool_use_id = old.tool_use_id.clone();
    }
}

fn keep_permission_visible(
    previous: &EnrichedAgentHookEventPayload,
    next: &AgentHookEventPayload,
) -> bool {
    let old = &previous.event;
    if previous.restored_unconfirmed
        || old.payload.agent_type.as_deref() != Some("claude")
        || old.payload.state != AgentStatusState::Waiting
        || old.hook_event_name.as_deref() != Some("PermissionRequest")
        || next.payload.agent_type.as_deref() != Some("claude")
        || next.payload.state != AgentStatusState::Working
        || next.has_explicit_prompt
        || crate::normalizers::ask_user::is_ask_user_question_tool(old.payload.tool_name.as_deref())
    {
        return false;
    }
    let old_agent = old
        .tool_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let next_agent = next
        .tool_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    if old_agent.is_some()
        && next.hook_event_name.as_deref() == Some("SubagentStop")
        && old_agent == next_agent
    {
        return false;
    }
    if next.hook_event_name.as_deref() == Some("TeammateIdle")
        && old_agent
            .zip(next.teammate_name.as_deref())
            .is_some_and(|(id, name)| {
                crate::normalizers::claude_roster::claude_teammate_id_matches_name(id, name)
            })
    {
        return false;
    }
    let old_tool = old
        .tool_use_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let next_tool = next
        .tool_use_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let matching_tool = old_tool.is_some() && old_tool == next_tool;
    let conflicting_tool = old_tool.is_some() && next_tool.is_some() && old_tool != next_tool;
    let matching_agent = old_agent.is_some() && old_agent == next_agent;
    let matching_type = old_agent.is_none()
        && next_agent.is_none()
        && old.tool_agent_type.is_some()
        && old.tool_agent_type == next.tool_agent_type;
    let matching_input =
        old.payload.tool_input.is_some() && old.payload.tool_input == next.payload.tool_input;
    let unknown_input = old.payload.tool_input.is_none()
        && next.payload.tool_input.is_none()
        && (matching_agent || matching_tool);
    !(matches!(
        next.hook_event_name.as_deref(),
        Some("PreToolUse" | "PostToolUse")
    ) && next_tool.is_some()
        && !conflicting_tool
        && (matching_agent || matching_type || matching_tool)
        && old.payload.tool_name.is_some()
        && old.payload.tool_name == next.payload.tool_name
        && (matching_input || unknown_input))
}

fn move_listener_pane(state: &mut HookListenerState, from: &str, to: &str) {
    macro_rules! move_map {
        ($($field:ident),*) => { $(if let Some(value) = state.$field.remove(from) { state.$field.insert(to.to_string(), value); })* };
    }
    macro_rules! move_set {
        ($($field:ident),*) => { $(if state.$field.remove(from) { state.$field.insert(to.to_string()); })* };
    }
    move_map!(
        last_prompt_by_pane_key,
        last_tool_by_pane_key,
        antigravity_completed_transcript_by_pane_key,
        claude_subagent_rosters,
        claude_lead_state_by_pane_key,
        codex_subagent_rosters,
        codex_lead_state_by_pane_key
    );
    move_set!(
        claude_unconfirmed_restored_status_pane_keys,
        claude_running_non_agent_task_pane_keys,
        claude_active_session_cron_pane_keys
    );
    state.amp_cache_order.move_pane(from, to);
    let prefix = format!("{from}\0");
    let prompt_keys: Vec<_> = state
        .last_prompt_by_pane_key
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .cloned()
        .collect();
    for key in prompt_keys {
        if let Some(value) = state.last_prompt_by_pane_key.remove(&key) {
            state
                .last_prompt_by_pane_key
                .insert(format!("{to}{}", &key[from.len()..]), value);
        }
    }
    let tool_keys: Vec<_> = state
        .last_tool_by_pane_key
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .cloned()
        .collect();
    for key in tool_keys {
        if let Some(value) = state.last_tool_by_pane_key.remove(&key) {
            state
                .last_tool_by_pane_key
                .insert(format!("{to}{}", &key[from.len()..]), value);
        }
    }
    let completed_keys: Vec<_> = state
        .amp_completed_cache_keys
        .iter()
        .filter(|key| key.starts_with(&prefix))
        .cloned()
        .collect();
    for key in completed_keys {
        state.amp_completed_cache_keys.remove(&key);
        state
            .amp_completed_cache_keys
            .insert(format!("{to}{}", &key[from.len()..]));
    }
}

fn clear_migration_unsupported_for_pane(
    registry: &mut HashMap<String, MigrationUnsupportedPtyEntry>,
    pane_key: &str,
    effects: &mut Vec<StoreEffect>,
) {
    let resolved: Vec<String> = registry
        .values()
        .filter(|entry| entry.pane_key.as_deref() == Some(pane_key))
        .map(|entry| entry.pty_id.clone())
        .collect();
    for pty_id in resolved {
        if registry.remove(&pty_id).is_some() {
            effects.push(StoreEffect::MigrationUnsupportedClear { pty_id });
        }
    }
}

/// `toAgentStatusIpcPayload` (`server.ts:438-454`).
pub fn to_ipc_payload(entry: &EnrichedAgentHookEventPayload) -> AgentStatusIpcPayload {
    AgentStatusIpcPayload {
        pane_key: entry.event.pane_key.clone(),
        launch_token: entry.event.launch_token.clone(),
        tab_id: entry.event.tab_id.clone(),
        worktree_id: entry.event.worktree_id.clone(),
        connection_id: entry.event.connection_id.clone(),
        received_at: entry.received_at,
        state_started_at: entry.state_started_at,
        provider_session: entry.event.provider_session.clone(),
        provider_session_only: entry.event.provider_session_only,
        prompt_interaction_key: entry.event.prompt_interaction_key.clone(),
        restored_unconfirmed: entry.restored_unconfirmed,
        observation: entry.observation.clone(),
        payload: entry.event.payload.clone(),
    }
}

pub fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Shared apply core over externally-owned state so inference can write back
/// through the same path (`applyNormalizedStatus` guards, local-host shape).
pub fn apply_enriched(
    state: &mut HookListenerState,
    entries: &mut HashMap<String, EnrichedAgentHookEventPayload>,
    mut event: AgentHookEventPayload,
    now: i64,
) -> Option<AgentStatusIpcPayload> {
    if !crate::types::is_valid_pane_key(&event.pane_key) {
        return None;
    }
    if event.hook_event_name.as_deref() == Some("UserPromptSubmit") {
        // The prompt boundary is authoritative even when text is unchanged.
    }
    let previous = entries.get(&event.pane_key).cloned();
    let restored_watermark = previous
        .as_ref()
        .filter(|prev| prev.restored_unconfirmed)
        .map(|prev| prev.received_at);
    // Why: renderer ordering rejects older rows; live evidence must sort after
    // restored rows across clock rollback.
    let effective_now = now.max(restored_watermark.map(|w| w.saturating_add(1)).unwrap_or(0));
    if event.provider_session_only {
        // Identity-only rows survive replay but must not emit a fabricated status.
        let enriched = EnrichedAgentHookEventPayload {
            restored_unconfirmed: event.restored_unconfirmed,
            event,
            claude_lead_boundary_child_only: false,
            retained_for_liveness: false,
            received_at: effective_now,
            state_started_at: effective_now,
            observation: None,
        };
        entries.insert(enriched.event.pane_key.clone(), enriched.clone());
        state
            .last_status_by_pane_key
            .insert(enriched.event.pane_key.clone(), enriched.event.clone());
        return Some(to_ipc_payload(&enriched));
    }
    if let Some(previous) = &previous {
        inherit_permission_tool_id(previous, &mut event);
        if keep_permission_visible(previous, &event) {
            return None;
        }
    }
    // Why: some TUIs emit a delayed tool/working hook after Ctrl+C stopped the
    // turn; don't let it resurrect the row.
    if let Some(prev) = &previous {
        let same_agent = prev.event.payload.agent_type == event.payload.agent_type;
        let same_prompt = prev.event.payload.prompt == event.payload.prompt;
        if prev.event.payload.state == AgentStatusState::Done
            && prev.event.payload.interrupted
            && same_agent
            && same_prompt
        {
            let within_suppression = effective_now.saturating_sub(prev.received_at)
                <= crate::types::INTERRUPTED_DONE_LATE_WORKING_SUPPRESSION_MS;
            if (event.payload.state == AgentStatusState::Done && within_suppression)
                || (event.payload.state == AgentStatusState::Working
                    && (event.is_replay
                        || is_tool_progress_after_interrupt(&event)
                        || (!event.has_explicit_prompt && within_suppression)))
            {
                // Suppressed: the previous row stands and nothing re-emits.
                return None;
            }
        }
    }
    let state_started_at = match &previous {
        Some(prev) if prev.event.payload.state == event.payload.state => prev.state_started_at,
        _ => effective_now,
    };
    let boundary = crate::normalizers::is_new_turn_event(
        event
            .source
            .unwrap_or(crate::types::AgentHookSource::Claude),
        event.hook_event_name.as_deref(),
    );
    let enriched = EnrichedAgentHookEventPayload {
        claude_lead_boundary_child_only: child_only_boundary(previous.as_ref(), &event),
        retained_for_liveness: false,
        restored_unconfirmed: event.restored_unconfirmed,
        event,
        received_at: effective_now,
        state_started_at,
        observation: None,
    };
    entries.insert(enriched.event.pane_key.clone(), enriched.clone());
    state
        .last_status_by_pane_key
        .insert(enriched.event.pane_key.clone(), enriched.event.clone());
    let _ = boundary;
    Some(to_ipc_payload(&enriched))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentHookSource, ParsedAgentStatusPayload};

    const UUID: &str = "3f2b8c64-9a1e-4c7d-8b2a-1f6e5d4c3b2a";

    fn working(pane: &str, prompt: &str) -> AgentHookEventPayload {
        AgentHookEventPayload {
            pane_key: pane.to_string(),
            source: Some(AgentHookSource::Claude),
            connection_id: None,
            payload: ParsedAgentStatusPayload {
                state: AgentStatusState::Working,
                prompt: prompt.to_string(),
                agent_type: Some("claude".into()),
                ..ParsedAgentStatusPayload::default()
            },
            ..AgentHookEventPayload::default()
        }
    }

    fn done(pane: &str, prompt: &str) -> AgentHookEventPayload {
        AgentHookEventPayload {
            pane_key: pane.to_string(),
            source: Some(AgentHookSource::Claude),
            connection_id: None,
            payload: ParsedAgentStatusPayload {
                state: AgentStatusState::Done,
                prompt: prompt.to_string(),
                agent_type: Some("claude".into()),
                ..ParsedAgentStatusPayload::default()
            },
            ..AgentHookEventPayload::default()
        }
    }

    #[test]
    fn restart_restores_claude_child_boundary_and_reaps_idle_teammates() {
        let dir =
            std::env::temp_dir().join(format!("mcode-as-claude-hydrate-{}", std::process::id()));
        let pane = format!("t1:{UUID}");
        let now = epoch_now();
        let mut store = StatusStore::new(Some(dir.clone()));
        let mut event = working(&pane, "lead finished");
        event.hook_event_name = Some("Stop".into());
        event.payload.turn_completed_at = Some(now - 500);
        event.payload.last_assistant_message = Some("Lead result".into());
        let child = |id: &str, state| crate::types::AgentSubagentSnapshot {
            id: id.into(),
            state,
            started_at: now - 1_000,
            agent_type: Some("worker".into()),
            description: Some("Investigate".into()),
            model: None,
        };
        event.payload.subagents = Some(vec![
            child("active-child", crate::types::AgentSubagentState::Working),
            child("idle-teammate", crate::types::AgentSubagentState::Idle),
        ]);
        store.apply(event, now - 100);
        assert!(store.entries()[&pane].claude_lead_boundary_child_only);
        store.persist_if_dirty().unwrap();
        let mut restored = StatusStore::new(Some(dir.clone()));
        assert!(restored.snapshot_ipc()[0].restored_unconfirmed);
        assert_eq!(
            restored.snapshot_ipc()[0]
                .payload
                .subagents
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        let lead = &restored.state().claude_lead_state_by_pane_key[&pane];
        assert_eq!(lead.state, AgentStatusState::Done);
        assert_eq!(lead.turn_completed_at, Some(now - 500));
        assert_eq!(
            restored.state().last_tool_by_pane_key[&pane]
                .last_assistant_message
                .as_deref(),
            Some("Lead result")
        );
        let roster = restored
            .state_mut()
            .claude_subagent_rosters
            .get_mut(&pane)
            .unwrap();
        assert_eq!(roster.len(), 1);
        assert!(roster["active-child"].restored_from_snapshot);
        assert!(roster["active-child"].background_tasks_authoritative);
        crate::normalizers::claude_roster::fold_claude_background_tasks_into_roster(
            roster,
            &[],
            now,
            true,
        );
        assert!(roster.is_empty());
        let mut fresh = working(&pane, "new turn");
        fresh.hook_event_name = Some("UserPromptSubmit".into());
        fresh.has_explicit_prompt = true;
        restored.apply(fresh, now);
        assert!(!restored.entries()[&pane].claude_lead_boundary_child_only);
        assert!(!restored.snapshot_ipc()[0].restored_unconfirmed);
        std::fs::remove_file(dir.join("last-status.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn normalizer_unconfirmed_child_evidence_does_not_become_live_in_store() {
        let pane = format!("t1:{UUID}");
        let mut store = StatusStore::new(None);
        let mut event = working(&pane, "restored child");
        event.restored_unconfirmed = true;
        store.apply(event, 1_000);
        assert!(store.snapshot_ipc()[0].restored_unconfirmed);
        assert!(!store.infer_interrupt(
            &crate::types::AgentInterruptInferenceRequest {
                pane_key: pane,
                baseline_updated_at: 1_000,
                baseline_state_started_at: 1_000,
                baseline_prompt: "restored child".into(),
                baseline_agent_type: Some("claude".into()),
                intent: "ctrl-c".into(),
                input_count: None
            },
            1_500
        ));
    }

    #[test]
    fn persistence_redacts_tokens_keeps_hash_across_hydration_and_retries_failure() {
        let dir = std::env::temp_dir().join(format!("mcode-as-redaction-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pane = format!("t1:{UUID}");
        let mut store = StatusStore::new(Some(dir.clone()));
        let mut event = working(&pane, "prompt");
        event.launch_token = Some("secret".into());
        event.prompt_interaction_key = Some("ephemeral-key".into());
        store.apply(event, epoch_now() - 100);
        assert!(store.persist_if_dirty().unwrap());
        let read = || {
            serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(dir.join("last-status.json")).unwrap(),
            )
            .unwrap()
        };
        let disk = read();
        let entry = &disk["entries"][&pane];
        assert_eq!(entry["source"], "claude");
        assert!(entry.get("launchToken").is_none());
        assert!(entry.get("promptInteractionKey").is_none());
        assert_eq!(
            entry["launchTokenHash"],
            "2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b"
        );
        assert_eq!(
            disk["authorityCommitments"][&pane]["launchTokenHash"],
            entry["launchTokenHash"]
        );
        let mut hydrated = StatusStore::new(Some(dir.clone()));
        assert_eq!(hydrated.state().last_prompt_by_pane_key[&pane], "prompt");
        hydrated.apply(done(&pane, "prompt"), epoch_now());
        assert!(hydrated.persist_if_dirty().unwrap());
        assert_eq!(
            read()["entries"][&pane]["launchTokenHash"],
            entry["launchTokenHash"]
        );
        std::fs::remove_file(dir.join("last-status.json")).unwrap();
        std::fs::create_dir(dir.join("last-status.json")).unwrap();
        hydrated.apply(working(&pane, "next"), epoch_now());
        assert!(hydrated.persist_if_dirty().is_err());
        std::fs::remove_dir(dir.join("last-status.json")).unwrap();
        assert!(hydrated.persist_if_dirty().unwrap());
        std::fs::remove_file(dir.join("last-status.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn transfer_rejects_unowned_pty_and_routes_late_hooks_to_new_owner() {
        let mut store = StatusStore::new(None);
        let from = format!("t1:{UUID}");
        let to = format!("t2:{UUID}");
        store.apply(working(&from, "first"), 1_000);
        store
            .state_mut()
            .last_prompt_by_pane_key
            .insert(from.clone(), "first".into());
        assert!(!store.can_transfer_pane_authority(&from, Some("unowned"), &|_, _| false));
        assert!(!store.can_transfer_pane_authority(&from, None, &|_, _| true));
        assert!(
            store.can_transfer_pane_authority(&from, Some("p1"), &|key, id| key == from
                && id == "p1")
        );
        assert!(store.transfer_pane_authority(&from, &to, Some("p1")));
        assert_eq!(store.state().last_prompt_by_pane_key[&to], "first");
        store.apply(done(&from, "first"), 2_000);
        assert_eq!(store.snapshot_ipc().len(), 1);
        assert_eq!(store.snapshot_ipc()[0].pane_key, to);
        assert_eq!(
            store.snapshot_ipc()[0].payload.state,
            AgentStatusState::Done
        );
    }

    #[test]
    fn restore_retired_transfer_restores_entire_alias_fence() {
        let mut store = StatusStore::new(None);
        let from = format!("t1:{UUID}");
        let to = format!("t2:{UUID}");
        store.apply(working(&from, "first"), 1_000);
        assert!(store.transfer_pane_authority(&from, &to, Some("p1")));
        store.retire_pane_authority(&to);
        assert!(!store.accepts_pane(&from));
        assert!(!store.accepts_pane(&to));
        assert!(store.restore_pane_authority(&to));
        assert!(store.accepts_pane(&from));
        assert_eq!(store.resolve_pane_key(&from), to);
        store.apply(working(&from, "second"), 2_000);
        assert_eq!(store.snapshot_ipc()[0].pane_key, to);
    }

    #[test]
    fn dismissal_clears_renderer_even_when_retaining_resume_identity() {
        let mut store = StatusStore::new(None);
        let pane = format!("t1:{UUID}");
        let mut event = working(&pane, "prompt");
        event.provider_session = Some(crate::types::AgentProviderSessionMetadata {
            key: "claude".into(),
            id: "session".into(),
            transcript_path: None,
        });
        store.apply(event, 1_000);
        let effects = store.drop_status_entry(&pane, 2_000);
        assert!(matches!(&effects[0], StoreEffect::Clear(_)));
        assert!(store.snapshot_ipc()[0].provider_session_only);
        assert_eq!(store.snapshot_ipc()[0].received_at, 1_000);
    }

    #[test]
    fn completed_turn_after_interrupt_is_only_suppressed_within_window() {
        let mut store = StatusStore::new(None);
        let pane = format!("t1:{UUID}");
        let mut interrupted = done(&pane, "prompt");
        interrupted.payload.interrupted = true;
        store.apply(interrupted, 1_000);
        assert!(store.apply(done(&pane, "prompt"), 2_000).is_empty());
        assert!(!store.apply(done(&pane, "prompt"), 20_000).is_empty());
    }

    #[test]
    fn permission_wait_survives_sibling_progress_until_owning_tool_resumes() {
        let mut store = StatusStore::new(None);
        let pane = format!("t1:{UUID}");
        let mut event = working(&pane, "prompt");
        event.hook_event_name = Some("PermissionRequest".into());
        event.payload.state = AgentStatusState::Waiting;
        event.payload.tool_name = Some("Bash".into());
        event.tool_use_id = Some("approved-tool".into());
        store.apply(event, 1_000);
        let mut sibling = working(&pane, "prompt");
        sibling.hook_event_name = Some("PostToolUse".into());
        sibling.tool_use_id = Some("sibling".into());
        sibling.payload.tool_name = Some("Bash".into());
        assert!(store.apply(sibling.clone(), 2_000).is_empty());
        sibling.tool_use_id = Some("approved-tool".into());
        assert!(!store.apply(sibling, 3_000).is_empty());
    }

    #[test]
    fn apply_sets_row_and_snapshot_projects() {
        let mut store = StatusStore::new(None);
        let effects = store.apply(working(&format!("t1:{UUID}"), "fix"), 1_000);
        assert_eq!(effects.len(), 1);
        let snapshot = store.snapshot_ipc();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].pane_key, format!("t1:{UUID}"));
        assert_eq!(snapshot[0].payload.state, AgentStatusState::Working);
        assert_eq!(snapshot[0].connection_id, None);
    }

    #[test]
    fn same_state_keeps_state_started_at() {
        let mut store = StatusStore::new(None);
        store.apply(working(&format!("t1:{UUID}"), "fix"), 1_000);
        store.apply(working(&format!("t1:{UUID}"), "fix"), 1_500);
        let snapshot = store.snapshot_ipc();
        assert_eq!(snapshot[0].state_started_at, 1_000);
        assert_eq!(snapshot[0].received_at, 1_500);
    }

    #[test]
    fn interrupted_done_suppresses_late_working() {
        let mut store = StatusStore::new(None);
        let now = crate::store::epoch_now();
        store.apply(working(&format!("t1:{UUID}"), "fix"), now - 60_000);
        store.apply(done(&format!("t1:{UUID}"), "fix"), now - 10_000);
        // Fabricate interrupted flag by direct entry surgery (inference does this).
        let key = format!("t1:{UUID}");
        let entry = store.entries.get_mut(&key).unwrap();
        entry.event.payload.interrupted = true;
        let effects = store.apply(working(&format!("t1:{UUID}"), "fix"), now - 5_000);
        let snapshot = store.snapshot_ipc();
        assert_eq!(snapshot[0].payload.state, AgentStatusState::Done);
        assert_eq!(effects.len(), 0);
    }

    #[test]
    fn drop_by_tab_prefix_clears_scoped_rows() {
        let mut store = StatusStore::new(None);
        store.apply(working(&format!("t1:{UUID}"), "a"), 1_000);
        store.apply(working(&format!("t2:{UUID}"), "b"), 1_000);
        let effects = store.drop_status_entries_by_tab_prefix("t1");
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], StoreEffect::Clear(_)));
        assert_eq!(store.snapshot_ipc().len(), 1);
    }

    #[test]
    fn invalid_pane_key_rejected() {
        let mut store = StatusStore::new(None);
        let effects = store.apply(working("bogus", "x"), 1_000);
        assert!(effects.is_empty());
        assert!(store.snapshot_ipc().is_empty());
    }

    #[test]
    fn persist_round_trip_v2() {
        let dir = std::env::temp_dir().join(format!("mcode-as-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut store = StatusStore::new(Some(dir.clone()));
            let now = crate::store::epoch_now();
            store.apply(working(&format!("t1:{UUID}"), "fix"), now - 2_000);
            store.apply(done(&format!("t1:{UUID}"), "fix"), now - 1_000);
            assert!(store.persist_if_dirty().unwrap());
        }
        {
            let store = StatusStore::new(Some(dir.clone()));
            let snapshot = store.snapshot_ipc();
            assert_eq!(snapshot.len(), 1);
            assert_eq!(snapshot[0].payload.state, AgentStatusState::Done);
            // Non-done rows hydrate as restoredUnconfirmed; done rows don't.
            assert!(!snapshot[0].restored_unconfirmed);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_mismatch_fails_closed() {
        let dir = std::env::temp_dir().join(format!("mcode-as-store2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("last-status.json"),
            r#"{"version":1,"entries":{}}"#,
        )
        .unwrap();
        let store = StatusStore::new(Some(dir.clone()));
        assert!(store.snapshot_ipc().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_unsupported_lifecycle() {
        let mut store = StatusStore::new(None);
        let entry = MigrationUnsupportedPtyEntry {
            pty_id: "p1".into(),
            pane_key: Some(format!("t1:{UUID}")),
            reason: "legacy-numeric-pane-key".into(),
            source: "local".into(),
            updated_at: 1,
            worktree_id: None,
            tab_id: None,
            leaf_id: None,
        };
        let effects = store.set_migration_unsupported(entry);
        assert_eq!(effects.len(), 1);
        assert_eq!(store.migration_unsupported_snapshot().len(), 1);
        let effects = store.clear_migration_unsupported_for_pane_key(&format!("t1:{UUID}"));
        assert_eq!(effects.len(), 1);
        assert!(store.migration_unsupported_snapshot().is_empty());
    }

    #[test]
    fn clear_pane_state_emits_clear_only_when_evicted() {
        let mut store = StatusStore::new(None);
        store.apply(working(&format!("t1:{UUID}"), "a"), 1_000);
        let effects = store.clear_pane_state(&format!("t1:{UUID}"));
        assert_eq!(effects.len(), 1);
        let effects = store.clear_pane_state(&format!("t1:{UUID}"));
        assert!(effects.is_empty());
    }

    #[test]
    fn user_prompt_submit_clears_turn_stamp() {
        let mut store = StatusStore::new(None);
        let mut event = done(&format!("t1:{UUID}"), "first");
        event.hook_event_name = Some("UserPromptSubmit".into());
        event.payload.state = AgentStatusState::Working;
        store.apply(event, 1_000);
        assert_eq!(
            store.snapshot_ipc()[0].payload.state,
            AgentStatusState::Working
        );
    }
}
