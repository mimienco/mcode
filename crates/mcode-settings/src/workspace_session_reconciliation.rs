use serde_json::{Map, Value};

pub(super) fn reconcile_local_workspace_session(
    incoming: &mut Value,
    prior: Option<&Value>,
    persisted: &Map<String, Value>,
) {
    let Some(incoming) = incoming.as_object_mut() else {
        return;
    };
    let Some(prior) = prior.and_then(Value::as_object) else {
        return;
    };

    super::workspace_session_topology::rebase_stale_terminal_membership(incoming, prior);
    super::workspace_session_bindings::preserve_tab_bindings(incoming, prior, persisted);
    super::workspace_session_bindings::preserve_layout_bindings(incoming, prior, persisted);
}
