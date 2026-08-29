use serde_json::{Map, Value};
use std::collections::HashSet;

use super::workspace_session_retirement_layouts::{
    first_layout_pty, group_active_tab, layout_contains_leaf, prune_group_layout,
    repair_tab_groups, retire_leaf_from_layout, visible_type,
};

pub(super) fn sanitize_terminal_retirements(
    incoming: &mut Map<String, Value>,
    prior: &Map<String, Value>,
) {
    if !prior.contains_key("terminalSurfaceTombstonesByPaneKey")
        && !incoming.contains_key("terminalSurfaceTombstonesByPaneKey")
        && !prior.contains_key("terminalTopologyRevisionByRepoId")
    {
        return;
    }
    let mut tombstones = record_field(prior, "terminalSurfaceTombstonesByPaneKey");
    tombstones.extend(record_field(incoming, "terminalSurfaceTombstonesByPaneKey"));

    if !tombstones.is_empty() {
        let mut incarnations = record_field(prior, "terminalPtyIncarnationsByPaneKey");
        incarnations.extend(record_field(incoming, "terminalPtyIncarnationsByPaneKey"));
        incoming.insert(
            "terminalPtyIncarnationsByPaneKey".into(),
            Value::Object(incarnations),
        );
        incoming.insert(
            "terminalSurfaceTombstonesByPaneKey".into(),
            Value::Object(tombstones.clone()),
        );
        for tombstone in tombstones.values() {
            retire_terminal_surface(incoming, tombstone);
        }
    }

    incoming.insert(
        "terminalSurfaceTombstonesByPaneKey".into(),
        Value::Object(Map::new()),
    );
    super::workspace_session_topology::rebase_stale_terminal_membership(incoming, prior);
}

fn retire_terminal_surface(session: &mut Map<String, Value>, tombstone: &Value) {
    let Some(tombstone) = tombstone.as_object() else {
        return;
    };
    let Some(worktree_id) = string_field(tombstone, "worktreeId") else {
        return;
    };
    let Some(parent_tab_id) = string_field(tombstone, "parentTabId") else {
        return;
    };
    let Some(leaf_id) = string_field(tombstone, "leafId") else {
        return;
    };
    let Some(pty_id) = string_field(tombstone, "ptyId") else {
        return;
    };
    let pane_key = format!("{parent_tab_id}:{leaf_id}");
    let bound_incarnation = session
        .get("terminalPtyIncarnationsByPaneKey")
        .and_then(Value::as_object)
        .and_then(|bindings| bindings.get(&pane_key))
        .and_then(Value::as_str);
    if let (Some(retired), Some(bound)) =
        (string_field(tombstone, "incarnationId"), bound_incarnation)
        && retired != bound
    {
        return;
    }

    let mut tabs_by_worktree = record_field(session, "tabsByWorktree");
    let persisted_tabs = array_field(&tabs_by_worktree, worktree_id);
    let persisted_tab = persisted_tabs
        .iter()
        .find(|tab| tab_id(tab) == Some(parent_tab_id));
    let mut layouts = record_field(session, "terminalLayoutsByTabId");
    let layout = layouts.get(parent_tab_id).cloned();
    let exact_leaf = layout
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|layout| layout_contains_leaf(layout.get("root"), leaf_id, 0));
    let leaf_pty_id = exact_leaf
        .then(|| {
            layout
                .as_ref()?
                .get("ptyIdsByLeafId")?
                .as_object()?
                .get(leaf_id)?
                .as_str()
        })
        .flatten();
    if leaf_pty_id.is_some_and(|bound| bound != pty_id) {
        return;
    }
    let legacy_final_surface = layout.is_none()
        && persisted_tab
            .and_then(|tab| tab.get("ptyId"))
            .and_then(Value::as_str)
            == Some(pty_id);
    if !exact_leaf && !legacy_final_surface {
        record_retirement(session, worktree_id, &pane_key);
        return;
    }

    let next_layout = if exact_leaf {
        layout
            .as_ref()
            .and_then(|layout| retire_leaf_from_layout(layout, leaf_id))
    } else {
        None
    };
    let remove_parent = next_layout.is_none();
    let next_tabs: Vec<Value> = if remove_parent {
        persisted_tabs
            .into_iter()
            .filter(|tab| tab_id(tab) != Some(parent_tab_id))
            .collect()
    } else {
        persisted_tabs
            .into_iter()
            .map(|mut tab| {
                if tab_id(&tab) == Some(parent_tab_id)
                    && let Some(tab) = tab.as_object_mut()
                {
                    tab.insert(
                        "ptyId".into(),
                        first_layout_pty(next_layout.as_ref()).map_or(Value::Null, Value::String),
                    );
                }
                tab
            })
            .collect()
    };
    tabs_by_worktree.insert(worktree_id.into(), Value::Array(next_tabs.clone()));
    session.insert("tabsByWorktree".into(), Value::Object(tabs_by_worktree));
    match next_layout {
        Some(layout) => {
            layouts.insert(parent_tab_id.into(), layout);
        }
        None => {
            layouts.remove(parent_tab_id);
        }
    }
    session.insert("terminalLayoutsByTabId".into(), Value::Object(layouts));

    let mut unified_tabs = record_field(session, "unifiedTabs");
    let has_unified_tabs = session.contains_key("unifiedTabs");
    let next_unified_tabs: Vec<Value> = array_field(&unified_tabs, worktree_id)
        .into_iter()
        .filter(|tab| {
            !remove_parent
                || (tab_id(tab) != Some(parent_tab_id)
                    && object_string(tab, "entityId") != Some(parent_tab_id))
        })
        .collect();
    if has_unified_tabs {
        unified_tabs.insert(worktree_id.into(), Value::Array(next_unified_tabs.clone()));
        session.insert("unifiedTabs".into(), Value::Object(unified_tabs));
    }

    let mut valid_ids = Vec::new();
    let mut valid_id_set = HashSet::new();
    for id in next_unified_tabs
        .iter()
        .chain(next_tabs.iter())
        .filter_map(tab_id)
    {
        if valid_id_set.insert(id.to_owned()) {
            valid_ids.push(id.to_owned());
        }
    }

    let mut groups_by_worktree = record_field(session, "tabGroups");
    let has_groups = session.contains_key("tabGroups");
    let repaired_groups = repair_tab_groups(
        array_field(&groups_by_worktree, worktree_id),
        &valid_id_set,
        worktree_id,
    );
    if has_groups {
        groups_by_worktree.insert(worktree_id.into(), Value::Array(repaired_groups.clone()));
        session.insert("tabGroups".into(), Value::Object(groups_by_worktree));
    }

    let retained_group_ids: HashSet<String> = repaired_groups
        .iter()
        .filter_map(|group| object_string(group, "id").map(str::to_owned))
        .collect();
    if session.contains_key("tabGroupLayouts") {
        let mut group_layouts = record_field(session, "tabGroupLayouts");
        match group_layouts
            .get(worktree_id)
            .and_then(|layout| prune_group_layout(layout, &retained_group_ids, 0))
        {
            Some(layout) => {
                group_layouts.insert(worktree_id.into(), layout);
            }
            None => {
                group_layouts.remove(worktree_id);
            }
        }
        session.insert("tabGroupLayouts".into(), Value::Object(group_layouts));
    }

    let previous_active_tab = session
        .get("activeTabIdByWorktree")
        .and_then(Value::as_object)
        .and_then(|record| record.get(worktree_id))
        .and_then(Value::as_str);
    let previous_active_group = session
        .get("activeGroupIdByWorktree")
        .and_then(Value::as_object)
        .and_then(|record| record.get(worktree_id))
        .and_then(Value::as_str);
    let next_active_tab = previous_active_tab
        .filter(|id| valid_id_set.contains(*id))
        .map(str::to_owned)
        .or_else(|| group_active_tab(&repaired_groups, previous_active_group))
        .or_else(|| valid_ids.first().cloned());
    let mut active_tabs = record_field(session, "activeTabIdByWorktree");
    active_tabs.insert(
        worktree_id.into(),
        next_active_tab.clone().map_or(Value::Null, Value::String),
    );
    session.insert("activeTabIdByWorktree".into(), Value::Object(active_tabs));

    if session.contains_key("activeTabTypeByWorktree") {
        let mut active_types = record_field(session, "activeTabTypeByWorktree");
        let active_type = next_active_tab.as_deref().and_then(|active| {
            next_unified_tabs
                .iter()
                .find(|tab| tab_id(tab) == Some(active))
                .and_then(|tab| visible_type(object_string(tab, "contentType")))
        });
        match (active_type, next_active_tab.as_ref()) {
            (Some(active_type), _) => {
                active_types.insert(worktree_id.into(), Value::String(active_type.into()));
            }
            (None, None) => {
                active_types.remove(worktree_id);
            }
            _ => {}
        }
        session.insert(
            "activeTabTypeByWorktree".into(),
            Value::Object(active_types),
        );
    }

    if session.contains_key("activeGroupIdByWorktree") {
        let mut active_groups = record_field(session, "activeGroupIdByWorktree");
        let active_group = repaired_groups
            .iter()
            .find(|group| {
                next_active_tab.as_deref().is_some_and(|active| {
                    group
                        .get("tabOrder")
                        .and_then(Value::as_array)
                        .is_some_and(|order| order.iter().any(|id| id.as_str() == Some(active)))
                })
            })
            .or_else(|| repaired_groups.first())
            .and_then(|group| object_string(group, "id"));
        match active_group {
            Some(group_id) => {
                active_groups.insert(worktree_id.into(), Value::String(group_id.into()));
            }
            None => {
                active_groups.remove(worktree_id);
            }
        }
        session.insert(
            "activeGroupIdByWorktree".into(),
            Value::Object(active_groups),
        );
    }

    if session.contains_key("remoteSessionIdsByTabId") {
        let mut remote_ids = record_field(session, "remoteSessionIdsByTabId");
        if remove_parent {
            remote_ids.remove(parent_tab_id);
        } else if remote_ids.get(parent_tab_id).and_then(Value::as_str) == Some(pty_id) {
            match first_layout_pty(
                session
                    .get("terminalLayoutsByTabId")
                    .and_then(Value::as_object)
                    .and_then(|layouts| layouts.get(parent_tab_id)),
            ) {
                Some(next_pty_id) => {
                    remote_ids.insert(parent_tab_id.into(), Value::String(next_pty_id));
                }
                None => {
                    remote_ids.remove(parent_tab_id);
                }
            }
        }
        session.insert("remoteSessionIdsByTabId".into(), Value::Object(remote_ids));
    }

    if session.get("activeTabId").and_then(Value::as_str) == Some(parent_tab_id) {
        session.insert(
            "activeTabId".into(),
            next_active_tab.map_or(Value::Null, Value::String),
        );
    }
    record_retirement(session, worktree_id, &pane_key);
}

fn record_retirement(session: &mut Map<String, Value>, worktree_id: &str, pane_key: &str) {
    let mut incarnations = record_field(session, "terminalPtyIncarnationsByPaneKey");
    incarnations.remove(pane_key);
    session.insert(
        "terminalPtyIncarnationsByPaneKey".into(),
        Value::Object(incarnations),
    );
    let mut tombstones = record_field(session, "terminalSurfaceTombstonesByPaneKey");
    tombstones.remove(pane_key);
    session.insert(
        "terminalSurfaceTombstonesByPaneKey".into(),
        Value::Object(tombstones),
    );
    let repo_id = repo_id_from_worktree_id(worktree_id);
    let mut revisions = record_field(session, "terminalTopologyRevisionByRepoId");
    let revision = revisions.get(repo_id).and_then(Value::as_u64).unwrap_or(0) + 1;
    revisions.insert(repo_id.into(), Value::from(revision));
    session.insert(
        "terminalTopologyRevisionByRepoId".into(),
        Value::Object(revisions),
    );
}

fn record_field(session: &Map<String, Value>, field: &str) -> Map<String, Value> {
    session
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn array_field(record: &Map<String, Value>, key: &str) -> Vec<Value> {
    record
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn tab_id(tab: &Value) -> Option<&str> {
    object_string(tab, "id")
}

fn object_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.as_object()?.get(field)?.as_str()
}

fn string_field<'a>(value: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    value.get(field)?.as_str()
}

fn repo_id_from_worktree_id(worktree_id: &str) -> &str {
    worktree_id
        .strip_prefix("worktree:")
        .unwrap_or(worktree_id)
        .split("::")
        .next()
        .unwrap_or(worktree_id)
}
