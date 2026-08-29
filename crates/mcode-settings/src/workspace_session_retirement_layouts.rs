use serde_json::Value;
use std::collections::HashSet;

pub(super) fn retire_leaf_from_layout(layout: &Value, retired_leaf_id: &str) -> Option<Value> {
    let mut layout = layout.as_object()?.clone();
    let root = prune_terminal_pane(layout.get("root")?, retired_leaf_id, 0)?;
    let leaf_ids = layout_leaf_ids(&root);
    layout.insert("root".into(), root);
    let active_leaf = layout
        .get("activeLeafId")
        .and_then(Value::as_str)
        .filter(|active| leaf_ids.iter().any(|leaf| leaf == active))
        .map(str::to_owned)
        .or_else(|| leaf_ids.first().cloned())?;
    layout.insert("activeLeafId".into(), Value::String(active_leaf));
    if layout
        .get("expandedLeafId")
        .and_then(Value::as_str)
        .is_some_and(|expanded| !leaf_ids.iter().any(|leaf| leaf == expanded))
    {
        layout.insert("expandedLeafId".into(), Value::Null);
    }
    for field in [
        "ptyIdsByLeafId",
        "buffersByLeafId",
        "scrollbackRefsByLeafId",
        "titlesByLeafId",
    ] {
        let Some(mut record) = layout.get(field).and_then(Value::as_object).cloned() else {
            continue;
        };
        record.remove(retired_leaf_id);
        if record.is_empty() {
            layout.remove(field);
        } else {
            layout.insert(field.into(), Value::Object(record));
        }
    }
    Some(Value::Object(layout))
}

fn prune_terminal_pane(node: &Value, retired_leaf_id: &str, depth: usize) -> Option<Value> {
    if depth > 128 {
        return None;
    }
    let fields = node.as_object()?;
    match fields.get("type").and_then(Value::as_str)? {
        "leaf" => (fields.get("leafId").and_then(Value::as_str) != Some(retired_leaf_id))
            .then(|| node.clone()),
        "split" => {
            let first = fields
                .get("first")
                .and_then(|child| prune_terminal_pane(child, retired_leaf_id, depth + 1));
            let second = fields
                .get("second")
                .and_then(|child| prune_terminal_pane(child, retired_leaf_id, depth + 1));
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

pub(super) fn repair_tab_groups(
    groups: Vec<Value>,
    valid_tab_ids: &HashSet<String>,
    worktree_id: &str,
) -> Vec<Value> {
    groups
        .into_iter()
        .filter_map(|group| {
            let mut group = group.as_object()?.clone();
            let order: Vec<Value> = group
                .get("tabOrder")?
                .as_array()?
                .iter()
                .filter(|id| id.as_str().is_some_and(|id| valid_tab_ids.contains(id)))
                .cloned()
                .collect();
            if order.is_empty() {
                return None;
            }
            let active = group
                .get("activeTabId")
                .and_then(Value::as_str)
                .filter(|active| order.iter().any(|id| id.as_str() == Some(*active)))
                .map(str::to_owned)
                .or_else(|| {
                    group
                        .get("recentTabIds")
                        .and_then(Value::as_array)
                        .and_then(|recent| {
                            recent.iter().rev().find_map(|id| {
                                id.as_str()
                                    .filter(|id| {
                                        order.iter().any(|entry| entry.as_str() == Some(*id))
                                    })
                                    .map(str::to_owned)
                            })
                        })
                })
                .or_else(|| order[0].as_str().map(str::to_owned));
            group.insert("tabOrder".into(), Value::Array(order.clone()));
            group.insert("worktreeId".into(), Value::String(worktree_id.into()));
            group.insert(
                "activeTabId".into(),
                active.map_or(Value::Null, Value::String),
            );
            if let Some(recent) = group.get("recentTabIds").and_then(Value::as_array) {
                let recent: Vec<Value> = recent
                    .iter()
                    .filter(|id| order.contains(id))
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

pub(super) fn group_active_tab(
    groups: &[Value],
    preferred_group_id: Option<&str>,
) -> Option<String> {
    groups
        .iter()
        .find(|group| object_string(group, "id") == preferred_group_id)
        .or_else(|| groups.first())
        .and_then(|group| object_string(group, "activeTabId"))
        .map(str::to_owned)
}

pub(super) fn prune_group_layout(
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
                .and_then(|node| prune_group_layout(node, retained_group_ids, depth + 1));
            let second = fields
                .get("second")
                .and_then(|node| prune_group_layout(node, retained_group_ids, depth + 1));
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

pub(super) fn layout_contains_leaf(root: Option<&Value>, leaf_id: &str, depth: usize) -> bool {
    if depth > 128 {
        return false;
    }
    let Some(fields) = root.and_then(Value::as_object) else {
        return false;
    };
    match fields.get("type").and_then(Value::as_str) {
        Some("leaf") => fields.get("leafId").and_then(Value::as_str) == Some(leaf_id),
        Some("split") => {
            layout_contains_leaf(fields.get("first"), leaf_id, depth + 1)
                || layout_contains_leaf(fields.get("second"), leaf_id, depth + 1)
        }
        _ => false,
    }
}

fn layout_leaf_ids(root: &Value) -> Vec<String> {
    fn visit(node: &Value, ids: &mut Vec<String>, depth: usize) {
        if depth > 128 {
            return;
        }
        let Some(fields) = node.as_object() else {
            return;
        };
        match fields.get("type").and_then(Value::as_str) {
            Some("leaf") => {
                if let Some(id) = fields.get("leafId").and_then(Value::as_str) {
                    ids.push(id.into());
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
    let mut ids = Vec::new();
    visit(root, &mut ids, 0);
    ids
}

pub(super) fn first_layout_pty(layout: Option<&Value>) -> Option<String> {
    let layout = layout?.as_object()?;
    let bindings = layout.get("ptyIdsByLeafId")?.as_object()?;
    layout
        .get("activeLeafId")
        .and_then(Value::as_str)
        .and_then(|active| bindings.get(active))
        .or_else(|| bindings.values().next())
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub(super) fn visible_type(content_type: Option<&str>) -> Option<&'static str> {
    match content_type {
        Some("terminal") => Some("terminal"),
        Some("browser") => Some("browser"),
        Some("simulator") => Some("simulator"),
        Some(_) => Some("editor"),
        None => None,
    }
}

fn object_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.as_object()?.get(field)?.as_str()
}
