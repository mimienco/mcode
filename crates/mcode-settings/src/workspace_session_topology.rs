use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub(super) fn rebase_stale_terminal_membership(
    incoming: &mut Map<String, Value>,
    prior: &Map<String, Value>,
) {
    let Some(prior_revisions) = prior
        .get("terminalTopologyRevisionByRepoId")
        .and_then(Value::as_object)
    else {
        return;
    };
    let incoming_revisions = incoming
        .get("terminalTopologyRevisionByRepoId")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut merged_revisions = incoming_revisions.clone();
    for (repo_id, prior_revision) in prior_revisions {
        let revision = prior_revision.as_u64().unwrap_or(0).max(
            merged_revisions
                .get(repo_id)
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
        merged_revisions.insert(repo_id.clone(), Value::from(revision));
    }

    let prior_tabs = record_field(prior, "tabsByWorktree");
    let mut tabs = record_field(incoming, "tabsByWorktree");
    let prior_layouts = record_field(prior, "terminalLayoutsByTabId");
    let mut layouts = record_field(incoming, "terminalLayoutsByTabId");
    let prior_unified_tabs = record_field(prior, "unifiedTabs");
    let mut unified_tabs = record_field(incoming, "unifiedTabs");
    let mut include_unified_tabs = incoming.contains_key("unifiedTabs");
    let prior_groups = record_field(prior, "tabGroups");
    let mut groups = record_field(incoming, "tabGroups");
    let mut include_groups = incoming.contains_key("tabGroups");
    let prior_group_layouts = record_field(prior, "tabGroupLayouts");
    let mut group_layouts = record_field(incoming, "tabGroupLayouts");
    let mut include_group_layouts = incoming.contains_key("tabGroupLayouts");
    let prior_active_tabs = record_field(prior, "activeTabIdByWorktree");
    let mut active_tabs = record_field(incoming, "activeTabIdByWorktree");
    let mut worktree_ids = HashSet::new();
    worktree_ids.extend(prior_tabs.keys().cloned());
    worktree_ids.extend(tabs.keys().cloned());
    let mut rebased = false;

    for worktree_id in worktree_ids {
        let repo_id = repo_id_from_worktree_id(&worktree_id);
        let prior_revision = prior_revisions
            .get(repo_id)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let incoming_revision = incoming_revisions
            .get(repo_id)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let revision = prior_revision.max(incoming_revision);
        if revision == 0 || incoming_revision > prior_revision {
            continue;
        }
        rebased = true;
        let current_tabs = array_field(&prior_tabs, &worktree_id);
        let candidate_tabs = array_field(&tabs, &worktree_id);
        let candidates_by_id: HashMap<String, Value> = candidate_tabs
            .iter()
            .filter_map(|tab| Some((tab_id(tab)?.into(), tab.clone())))
            .collect();
        let terminal_tab_ids: HashSet<String> = current_tabs
            .iter()
            .filter_map(|tab| tab_id(tab).map(str::to_owned))
            .collect();
        let rebased_tabs: Vec<Value> = current_tabs
            .iter()
            .map(|current| {
                let Some(current_id) = tab_id(current) else {
                    return current.clone();
                };
                let Some(candidate) = candidates_by_id.get(current_id) else {
                    return current.clone();
                };
                let mut candidate = candidate.as_object().cloned().unwrap_or_default();
                candidate.insert(
                    "ptyId".into(),
                    current.get("ptyId").cloned().unwrap_or(Value::Null),
                );
                Value::Object(candidate)
            })
            .collect();
        for candidate in &candidate_tabs {
            if let Some(candidate_id) = tab_id(candidate)
                && !terminal_tab_ids.contains(candidate_id)
            {
                layouts.remove(candidate_id);
            }
        }
        for tab_id in &terminal_tab_ids {
            match rebase_layout(layouts.get(tab_id), prior_layouts.get(tab_id)) {
                Some(layout) => {
                    layouts.insert(tab_id.clone(), layout);
                }
                None => {
                    layouts.remove(tab_id);
                }
            }
        }

        let rebased_unified_tabs = rebase_unified_tabs(
            array_field(&unified_tabs, &worktree_id),
            array_field(&prior_unified_tabs, &worktree_id),
            &terminal_tab_ids,
        );
        let mut valid_tab_ids = terminal_tab_ids.clone();
        valid_tab_ids.extend(
            rebased_unified_tabs
                .iter()
                .filter_map(|tab| tab_id(tab).map(str::to_owned)),
        );
        let candidate_groups = if groups.contains_key(&worktree_id) {
            array_field(&groups, &worktree_id)
        } else {
            array_field(&prior_groups, &worktree_id)
        };
        let rebased_groups = rebase_tab_groups(candidate_groups, &valid_tab_ids);
        let retained_group_ids: HashSet<String> = rebased_groups
            .iter()
            .filter_map(|group| object_string(group, "id").map(str::to_owned))
            .collect();
        let candidate_group_layout = group_layouts
            .get(&worktree_id)
            .or_else(|| prior_group_layouts.get(&worktree_id));
        match candidate_group_layout
            .and_then(|layout| prune_tab_group_layout(layout, &retained_group_ids, 0))
        {
            Some(layout) => {
                group_layouts.insert(worktree_id.clone(), layout);
                include_group_layouts = true;
            }
            None => {
                group_layouts.remove(&worktree_id);
            }
        }
        let active_tab_id = active_tabs
            .get(&worktree_id)
            .and_then(Value::as_str)
            .filter(|tab_id| valid_tab_ids.contains(*tab_id))
            .map(str::to_owned)
            .or_else(|| {
                prior_active_tabs
                    .get(&worktree_id)
                    .and_then(Value::as_str)
                    .filter(|tab_id| valid_tab_ids.contains(*tab_id))
                    .map(str::to_owned)
            })
            .or_else(|| {
                rebased_groups
                    .first()
                    .and_then(|group| object_string(group, "activeTabId"))
                    .map(str::to_owned)
            })
            .or_else(|| terminal_tab_ids.iter().next().cloned());

        tabs.insert(worktree_id.clone(), Value::Array(rebased_tabs));
        if include_unified_tabs || !rebased_unified_tabs.is_empty() {
            unified_tabs.insert(worktree_id.clone(), Value::Array(rebased_unified_tabs));
            include_unified_tabs = true;
        }
        if include_groups || !rebased_groups.is_empty() {
            groups.insert(worktree_id.clone(), Value::Array(rebased_groups));
            include_groups = true;
        }
        active_tabs.insert(
            worktree_id,
            active_tab_id.map_or(Value::Null, Value::String),
        );
    }

    incoming.insert(
        "terminalTopologyRevisionByRepoId".into(),
        Value::Object(merged_revisions),
    );
    if !rebased {
        return;
    }
    incoming.insert("tabsByWorktree".into(), Value::Object(tabs));
    incoming.insert("terminalLayoutsByTabId".into(), Value::Object(layouts));
    if include_unified_tabs {
        incoming.insert("unifiedTabs".into(), Value::Object(unified_tabs));
    }
    if include_groups {
        incoming.insert("tabGroups".into(), Value::Object(groups));
    }
    if include_group_layouts {
        incoming.insert("tabGroupLayouts".into(), Value::Object(group_layouts));
    }
    incoming.insert("activeTabIdByWorktree".into(), Value::Object(active_tabs));
    rebase_incarnation_bindings(incoming, prior);
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

fn rebase_layout(candidate: Option<&Value>, current: Option<&Value>) -> Option<Value> {
    let current = current?.as_object()?;
    let Some(candidate) = candidate.and_then(Value::as_object) else {
        return Some(Value::Object(current.clone()));
    };
    if layout_leaf_ids(candidate.get("root")) != layout_leaf_ids(current.get("root")) {
        return Some(Value::Object(current.clone()));
    }
    let mut rebased = candidate.clone();
    let mut bindings = candidate
        .get("ptyIdsByLeafId")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(current_bindings) = current.get("ptyIdsByLeafId").and_then(Value::as_object) {
        bindings.extend(current_bindings.clone());
    }
    rebased.insert("ptyIdsByLeafId".into(), Value::Object(bindings));
    Some(Value::Object(rebased))
}

fn layout_leaf_ids(root: Option<&Value>) -> HashSet<String> {
    fn visit(node: &Value, ids: &mut HashSet<String>, depth: usize) {
        if depth > 128 {
            return;
        }
        let Some(fields) = node.as_object() else {
            return;
        };
        match fields.get("type").and_then(Value::as_str) {
            Some("leaf") => {
                if let Some(leaf_id) = fields.get("leafId").and_then(Value::as_str) {
                    ids.insert(leaf_id.into());
                }
            }
            Some("split") => {
                if let Some(first) = fields.get("first") {
                    visit(first, ids, depth + 1);
                }
                if let Some(second) = fields.get("second") {
                    visit(second, ids, depth + 1);
                }
            }
            _ => {}
        }
    }
    let mut ids = HashSet::new();
    if let Some(root) = root.filter(|root| !root.is_null()) {
        visit(root, &mut ids, 0);
    }
    ids
}

fn terminal_unified_tab_matches(tab: &Value, terminal_tab_ids: &HashSet<String>) -> bool {
    object_string(tab, "contentType") == Some("terminal")
        && [object_string(tab, "id"), object_string(tab, "entityId")]
            .into_iter()
            .flatten()
            .any(|id| terminal_tab_ids.contains(id))
}

fn rebase_unified_tabs(
    candidate: Vec<Value>,
    current: Vec<Value>,
    terminal_tab_ids: &HashSet<String>,
) -> Vec<Value> {
    let mut result: Vec<Value> = candidate
        .into_iter()
        .filter(|tab| {
            object_string(tab, "contentType") != Some("terminal")
                || terminal_unified_tab_matches(tab, terminal_tab_ids)
        })
        .collect();
    let mut represented = HashSet::new();
    for tab in &result {
        if object_string(tab, "contentType") == Some("terminal") {
            represented.extend(
                [object_string(tab, "id"), object_string(tab, "entityId")]
                    .into_iter()
                    .flatten()
                    .map(str::to_owned),
            );
        }
    }
    for tab in current {
        if terminal_unified_tab_matches(&tab, terminal_tab_ids)
            && [object_string(&tab, "id"), object_string(&tab, "entityId")]
                .into_iter()
                .flatten()
                .all(|id| !represented.contains(id))
        {
            result.push(tab);
        }
    }
    result
}

fn rebase_tab_groups(groups: Vec<Value>, valid_tab_ids: &HashSet<String>) -> Vec<Value> {
    groups
        .into_iter()
        .filter_map(|group| {
            let mut group = group.as_object()?.clone();
            let tab_order: Vec<Value> = group
                .get("tabOrder")?
                .as_array()?
                .iter()
                .filter(|tab_id| {
                    tab_id
                        .as_str()
                        .is_some_and(|tab_id| valid_tab_ids.contains(tab_id))
                })
                .cloned()
                .collect();
            if tab_order.is_empty() {
                return None;
            }
            let active_tab_id = group
                .get("activeTabId")
                .and_then(Value::as_str)
                .filter(|active| {
                    tab_order
                        .iter()
                        .any(|tab_id| tab_id.as_str() == Some(*active))
                })
                .map(str::to_owned)
                .or_else(|| tab_order[0].as_str().map(str::to_owned));
            group.insert("tabOrder".into(), Value::Array(tab_order.clone()));
            group.insert(
                "activeTabId".into(),
                active_tab_id.map_or(Value::Null, Value::String),
            );
            if let Some(recent) = group.get("recentTabIds").and_then(Value::as_array) {
                let recent: Vec<Value> = recent
                    .iter()
                    .filter(|tab_id| tab_order.contains(tab_id))
                    .cloned()
                    .collect();
                if recent.is_empty() {
                    group.remove("recentTabIds");
                } else {
                    group.insert("recentTabIds".into(), Value::Array(recent));
                }
            }
            Some(Value::Object(group))
        })
        .collect()
}

fn prune_tab_group_layout(
    layout: &Value,
    retained_group_ids: &HashSet<String>,
    depth: usize,
) -> Option<Value> {
    if depth > 128 {
        return None;
    }
    let fields = layout.as_object()?;
    match fields.get("type").and_then(Value::as_str)? {
        "leaf" => retained_group_ids
            .contains(fields.get("groupId")?.as_str()?)
            .then(|| layout.clone()),
        "split" => {
            let first = fields
                .get("first")
                .and_then(|node| prune_tab_group_layout(node, retained_group_ids, depth + 1));
            let second = fields
                .get("second")
                .and_then(|node| prune_tab_group_layout(node, retained_group_ids, depth + 1));
            match (first, second) {
                (Some(first), Some(second)) => {
                    let mut split = fields.clone();
                    split.insert("first".into(), first);
                    split.insert("second".into(), second);
                    Some(Value::Object(split))
                }
                (Some(node), None) | (None, Some(node)) => Some(node),
                (None, None) => None,
            }
        }
        _ => None,
    }
}

fn rebase_incarnation_bindings(incoming: &mut Map<String, Value>, prior: &Map<String, Value>) {
    let terminal_tab_ids: HashSet<String> = record_field(incoming, "tabsByWorktree")
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|tab| tab_id(tab).map(str::to_owned))
        .collect();
    let layouts = record_field(incoming, "terminalLayoutsByTabId");
    let mut allowed_pane_keys = HashSet::new();
    for tab_id in &terminal_tab_ids {
        if let Some(layout) = layouts.get(tab_id).and_then(Value::as_object) {
            for leaf_id in layout_leaf_ids(layout.get("root")) {
                allowed_pane_keys.insert(format!("{tab_id}:{leaf_id}"));
            }
        }
    }
    let mut merged = record_field(incoming, "terminalPtyIncarnationsByPaneKey");
    merged.extend(record_field(prior, "terminalPtyIncarnationsByPaneKey"));
    merged.retain(|pane_key, _| {
        let Some((tab_id, _)) = pane_key.rsplit_once(':') else {
            return false;
        };
        if layouts.contains_key(tab_id) {
            allowed_pane_keys.contains(pane_key)
        } else {
            terminal_tab_ids.contains(tab_id)
        }
    });
    if merged.is_empty() {
        incoming.remove("terminalPtyIncarnationsByPaneKey");
    } else {
        incoming.insert(
            "terminalPtyIncarnationsByPaneKey".into(),
            Value::Object(merged),
        );
    }
}

fn repo_id_from_worktree_id(worktree_id: &str) -> &str {
    worktree_id
        .strip_prefix("worktree:")
        .unwrap_or(worktree_id)
        .split("::")
        .next()
        .unwrap_or(worktree_id)
}
