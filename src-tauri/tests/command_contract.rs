use mcode_app::{BuildLane, LifecycleService};
use mcode_desktop::app_get_launch_context_for;
use serde_json::json;

#[test]
fn launch_context_command_serializes_the_renderer_contract() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);

    let value = serde_json::to_value(app_get_launch_context_for(&service)).unwrap();

    assert_eq!(
        value,
        json!({
            "version": "1.4.178",
            "buildLane": "development",
            "headless": false,
            "safeMode": false
        })
    );
}
