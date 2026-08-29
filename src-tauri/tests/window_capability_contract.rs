use serde_json::Value;
use std::collections::BTreeSet;

#[test]
fn main_window_grants_only_default_and_required_window_mutations() {
    let capability: Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    let permissions = capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|permission| permission.as_str().unwrap())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        permissions,
        BTreeSet::from([
            "core:default",
            "core:window:allow-close",
            "core:window:allow-minimize",
            "core:window:allow-toggle-maximize",
        ])
    );
}
