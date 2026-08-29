use serde_json::{Map, Value};

pub(super) fn reconcile_local_workspace_session(
    incoming: &mut Value,
    prior: Option<&Value>,
    persisted: &Map<String, Value>,
) {
    let Some(incoming) = incoming.as_object_mut() else {
        return;
    };
    let empty_prior = Map::new();
    let prior = prior.and_then(Value::as_object).unwrap_or(&empty_prior);

    super::workspace_session_retirements::sanitize_terminal_retirements(incoming, prior);
    if prior.is_empty() {
        return;
    }
    super::workspace_session_bindings::preserve_tab_bindings(incoming, prior, persisted);
    super::workspace_session_bindings::preserve_layout_bindings(incoming, prior, persisted);
}

pub(super) fn reconcile_host_workspace_session(incoming: &mut Value, prior: Option<&Value>) {
    let Some(incoming) = incoming.as_object_mut() else {
        return;
    };
    let empty_prior = Map::new();
    let prior = prior.and_then(Value::as_object).unwrap_or(&empty_prior);

    super::workspace_session_retirements::sanitize_terminal_retirements(incoming, prior);
}
