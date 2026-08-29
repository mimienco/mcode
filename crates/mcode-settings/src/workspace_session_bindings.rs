use super::decode_uri_component;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

struct PtyBinding<'a> {
    pty_id: &'a str,
    target_id: Option<&'a str>,
    worktree_id: Option<&'a str>,
    tab_id: Option<&'a str>,
    leaf_id: Option<&'a str>,
}

pub(super) fn preserve_tab_bindings(
    incoming: &mut Map<String, Value>,
    prior: &Map<String, Value>,
    persisted: &Map<String, Value>,
) {
    let prior_tabs = prior
        .get("tabsByWorktree")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(incoming_tabs) = incoming
        .get_mut("tabsByWorktree")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    for (worktree_id, tabs) in incoming_tabs {
        let Some(tabs) = tabs.as_array_mut() else {
            continue;
        };
        let Some(prior_tabs) = prior_tabs.get(worktree_id).and_then(Value::as_array) else {
            continue;
        };
        let target_id = connection_id_for_worktree(persisted, worktree_id);
        for tab in tabs {
            let Some(tab) = tab.as_object_mut() else {
                continue;
            };
            if !tab.get("ptyId").is_some_and(Value::is_null) {
                continue;
            }
            let Some(tab_id) = tab.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(prior_pty_id) = prior_tabs.iter().find_map(|candidate| {
                let fields = candidate.as_object()?;
                (fields.get("id").and_then(Value::as_str) == Some(tab_id))
                    .then(|| fields.get("ptyId").and_then(Value::as_str))
                    .flatten()
            }) else {
                continue;
            };
            if is_restorable_binding(
                persisted,
                PtyBinding {
                    pty_id: prior_pty_id,
                    target_id: target_id.as_deref(),
                    worktree_id: Some(worktree_id),
                    tab_id: Some(tab_id),
                    leaf_id: None,
                },
            ) {
                tab.insert("ptyId".into(), Value::String(prior_pty_id.into()));
            }
        }
    }
}

pub(super) fn preserve_layout_bindings(
    incoming: &mut Map<String, Value>,
    prior: &Map<String, Value>,
    persisted: &Map<String, Value>,
) {
    let worktree_id_by_tab_id = index_tab_worktrees(prior, incoming);
    let prior_layouts = prior
        .get("terminalLayoutsByTabId")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(incoming_layouts) = incoming
        .get_mut("terminalLayoutsByTabId")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    for (tab_id, layout) in incoming_layouts {
        let Some(layout) = layout.as_object_mut() else {
            continue;
        };
        let Some(prior_layout) = prior_layouts.get(tab_id).and_then(Value::as_object) else {
            continue;
        };
        restore_missing_root(layout, prior_layout);

        let Some(prior_bindings) = prior_layout
            .get("ptyIdsByLeafId")
            .and_then(Value::as_object)
        else {
            continue;
        };
        let incoming_bindings = layout
            .get("ptyIdsByLeafId")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let incoming_has_any_binding = !incoming_bindings.is_empty();
        let live_leaf_ids = collect_live_leaf_ids(layout.get("root"));
        let worktree_id = worktree_id_by_tab_id.get(tab_id).map(String::as_str);
        let target_id = worktree_id.and_then(|id| connection_id_for_worktree(persisted, id));
        let mut restorable = Map::new();

        for (leaf_id, pty_id) in prior_bindings {
            let Some(pty_id) = pty_id.as_str() else {
                continue;
            };
            if !live_leaf_ids.contains(leaf_id) || incoming_bindings.contains_key(leaf_id) {
                continue;
            }
            let binding = PtyBinding {
                pty_id,
                target_id: target_id.as_deref(),
                worktree_id,
                tab_id: Some(tab_id),
                leaf_id: Some(leaf_id),
            };
            let may_restore = if incoming_has_any_binding {
                has_restorable_ssh_lease(persisted, binding)
            } else {
                is_restorable_binding(persisted, binding)
            };
            if may_restore {
                restorable.insert(leaf_id.clone(), Value::String(pty_id.into()));
            }
        }

        if restorable.is_empty() {
            continue;
        }
        restorable.extend(incoming_bindings);
        layout.insert("ptyIdsByLeafId".into(), Value::Object(restorable));
        for field in [
            "buffersByLeafId",
            "scrollbackRefsByLeafId",
            "titlesByLeafId",
        ] {
            preserve_missing_leaf_record(layout, prior_layout, field, &live_leaf_ids);
        }
    }
}

fn index_tab_worktrees(
    prior: &Map<String, Value>,
    incoming: &Map<String, Value>,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for session in [prior, incoming] {
        let Some(tabs_by_worktree) = session.get("tabsByWorktree").and_then(Value::as_object)
        else {
            continue;
        };
        for (worktree_id, tabs) in tabs_by_worktree {
            let Some(tabs) = tabs.as_array() else {
                continue;
            };
            for tab in tabs {
                if let Some(tab_id) = tab
                    .as_object()
                    .and_then(|fields| fields.get("id"))
                    .and_then(Value::as_str)
                {
                    result.insert(tab_id.into(), worktree_id.clone());
                }
            }
        }
    }
    result
}

fn restore_missing_root(layout: &mut Map<String, Value>, prior: &Map<String, Value>) {
    if !layout.get("root").is_some_and(Value::is_null) {
        return;
    }
    let Some(root) = prior.get("root").filter(|root| !root.is_null()).cloned() else {
        return;
    };
    let live_leaf_ids = collect_live_leaf_ids(Some(&root));
    let first_leaf_id = first_layout_leaf_id(&root);
    layout.insert("root".into(), root);
    restore_layout_selection(layout, prior, "activeLeafId", &live_leaf_ids, first_leaf_id);
    restore_layout_selection(layout, prior, "expandedLeafId", &live_leaf_ids, None);
}

fn restore_layout_selection(
    layout: &mut Map<String, Value>,
    prior: &Map<String, Value>,
    field: &str,
    live_leaf_ids: &HashSet<String>,
    fallback: Option<String>,
) {
    let selected = layout
        .get(field)
        .and_then(Value::as_str)
        .filter(|leaf_id| live_leaf_ids.contains(*leaf_id))
        .map(str::to_owned)
        .or_else(|| {
            prior
                .get(field)
                .and_then(Value::as_str)
                .filter(|leaf_id| live_leaf_ids.contains(*leaf_id))
                .map(str::to_owned)
        })
        .or(fallback);
    layout.insert(field.into(), selected.map_or(Value::Null, Value::String));
}

fn preserve_missing_leaf_record(
    layout: &mut Map<String, Value>,
    prior: &Map<String, Value>,
    field: &str,
    live_leaf_ids: &HashSet<String>,
) {
    let prior_record = prior
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let incoming_record = layout
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut next = Map::new();
    for (leaf_id, value) in prior_record {
        if live_leaf_ids.contains(&leaf_id) && !incoming_record.contains_key(&leaf_id) {
            next.insert(leaf_id, value);
        }
    }
    next.extend(incoming_record);
    if !next.is_empty() {
        layout.insert(field.into(), Value::Object(next));
    }
}

fn collect_live_leaf_ids(root: Option<&Value>) -> HashSet<String> {
    fn visit(node: &Value, leaf_ids: &mut HashSet<String>, depth: usize) {
        if depth > 128 {
            return;
        }
        let Some(fields) = node.as_object() else {
            return;
        };
        match fields.get("type").and_then(Value::as_str) {
            Some("leaf") => {
                if let Some(leaf_id) = fields.get("leafId").and_then(Value::as_str)
                    && is_terminal_leaf_id(leaf_id)
                {
                    leaf_ids.insert(leaf_id.into());
                }
            }
            Some("split") => {
                if let Some(first) = fields.get("first") {
                    visit(first, leaf_ids, depth + 1);
                }
                if let Some(second) = fields.get("second") {
                    visit(second, leaf_ids, depth + 1);
                }
            }
            _ => {}
        }
    }

    let mut leaf_ids = HashSet::new();
    if let Some(root) = root.filter(|root| !root.is_null()) {
        visit(root, &mut leaf_ids, 0);
    }
    leaf_ids
}

fn first_layout_leaf_id(root: &Value) -> Option<String> {
    let fields = root.as_object()?;
    match fields.get("type").and_then(Value::as_str)? {
        "leaf" => fields.get("leafId")?.as_str().map(str::to_owned),
        "split" => first_layout_leaf_id(fields.get("first")?),
        _ => None,
    }
}

fn is_terminal_leaf_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && matches!(bytes[14], b'1'..=b'5')
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || matches!(byte, b'0'..=b'9' | b'a'..=b'f')
        })
}

fn connection_id_for_worktree(persisted: &Map<String, Value>, worktree_id: &str) -> Option<String> {
    let worktree_id = worktree_id.strip_prefix("worktree:").unwrap_or(worktree_id);
    let repo_id = worktree_id.split("::").next()?;
    persisted.get("repos")?.as_array()?.iter().find_map(|repo| {
        let repo = repo.as_object()?;
        (repo.get("id").and_then(Value::as_str) == Some(repo_id))
            .then(|| repo.get("connectionId").and_then(Value::as_str))
            .flatten()
            .map(str::to_owned)
    })
}

fn is_restorable_binding(persisted: &Map<String, Value>, binding: PtyBinding<'_>) -> bool {
    !matching_leases(persisted, binding).any(|lease| {
        matches!(
            lease.get("state").and_then(Value::as_str),
            Some("terminated" | "expired")
        )
    })
}

fn has_restorable_ssh_lease(persisted: &Map<String, Value>, binding: PtyBinding<'_>) -> bool {
    matching_leases(persisted, binding).any(|lease| {
        !matches!(
            lease.get("state").and_then(Value::as_str),
            Some("terminated" | "expired")
        )
    })
}

fn matching_leases<'a>(
    persisted: &'a Map<String, Value>,
    binding: PtyBinding<'a>,
) -> impl Iterator<Item = &'a Map<String, Value>> {
    persisted
        .get("sshRemotePtyLeases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(move |lease| lease_matches_binding(lease, &binding))
}

fn lease_matches_binding(lease: &Map<String, Value>, binding: &PtyBinding<'_>) -> bool {
    let Some(target_id) = lease.get("targetId").and_then(Value::as_str) else {
        return false;
    };
    let Some(lease_pty_id) = lease.get("ptyId").and_then(Value::as_str) else {
        return false;
    };
    comparable_pty_id(target_id, binding.pty_id) == lease_pty_id
        && binding.target_id.is_none_or(|value| value == target_id)
        && optional_lease_identity_matches(lease, "worktreeId", binding.worktree_id)
        && optional_lease_identity_matches(lease, "tabId", binding.tab_id)
        && optional_lease_identity_matches(lease, "leafId", binding.leaf_id)
}

fn optional_lease_identity_matches(
    lease: &Map<String, Value>,
    field: &str,
    expected: Option<&str>,
) -> bool {
    expected.is_none_or(|value| lease.get(field).and_then(Value::as_str) == Some(value))
}

fn comparable_pty_id<'a>(target_id: &str, pty_id: &'a str) -> &'a str {
    let Some(rest) = pty_id.strip_prefix("ssh:") else {
        return pty_id;
    };
    let Some(separator) = rest.find("@@") else {
        return pty_id;
    };
    let (encoded_target_id, relay_with_separator) = rest.split_at(separator);
    let relay_id = &relay_with_separator[2..];
    if !relay_id.is_empty() && decode_uri_component(encoded_target_id).as_deref() == Some(target_id)
    {
        relay_id
    } else {
        pty_id
    }
}
