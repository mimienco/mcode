use serde_json::Value;
use std::collections::BTreeSet;

#[test]
fn bundler_creates_signed_updater_artifacts() {
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();

    assert_eq!(config["bundle"]["createUpdaterArtifacts"], true);
}

#[test]
fn renderer_cannot_bypass_the_narrow_update_commands_with_raw_plugin_permissions() {
    let capability: Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    let permissions = capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|permission| permission.as_str().unwrap())
        .collect::<BTreeSet<_>>();

    assert!(
        permissions
            .iter()
            .all(|permission| !permission.starts_with("updater:"))
    );
}

#[test]
fn static_plugin_configuration_cannot_be_used_as_an_unvalidated_fallback_feed() {
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();

    assert_eq!(config["plugins"]["updater"]["pubkey"], "");
    assert_eq!(
        config["plugins"]["updater"]["endpoints"],
        serde_json::json!([])
    );
}

#[test]
fn release_build_recompiles_when_any_native_feed_input_changes() {
    let build_script = include_str!("../build.rs");

    for variable in [
        "MCODE_TAURI_UPDATER_PUBLIC_KEY",
        "MCODE_TAURI_UPDATER_STABLE_ENDPOINT",
        "MCODE_TAURI_UPDATER_RC_ENDPOINT",
        "MCODE_TAURI_UPDATER_HOURLY_ENDPOINT",
        "MCODE_TAURI_UPDATER_DAILY_ENDPOINT",
        "MCODE_TAURI_UPDATER_ADHOC_ENDPOINT",
    ] {
        assert!(build_script.contains(variable));
    }
}
