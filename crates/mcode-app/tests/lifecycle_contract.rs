use mcode_app::{BuildLane, LaunchContext, LifecycleService};

#[test]
fn desktop_launch_context_exposes_identity_without_host_capabilities() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);

    assert_eq!(
        service.launch_context(),
        LaunchContext {
            version: "1.4.178".to_owned(),
            build_lane: BuildLane::Development,
            headless: false,
            safe_mode: false,
        }
    );
}
