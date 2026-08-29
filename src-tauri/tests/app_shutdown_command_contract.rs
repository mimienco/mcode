use mcode_app::{BuildLane, LifecycleService, ShutdownIntent};
use mcode_desktop::{
    app_cancel_shutdown_for, app_commit_shutdown_for, app_confirm_shutdown_for,
    app_request_shutdown_for, app_should_allow_exit_for,
};
use serde_json::json;

#[test]
fn shutdown_commands_preserve_the_versioned_renderer_contract() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);

    let request = app_request_shutdown_for(&service, ShutdownIntent::Restart).unwrap();

    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({
            "version": 1,
            "requestId": "1",
            "intent": "restart"
        })
    );
    assert_eq!(
        app_confirm_shutdown_for(&service, &request.request_id).unwrap(),
        request
    );
    assert_eq!(
        app_commit_shutdown_for(&service, &request.request_id).unwrap(),
        request
    );
}

#[test]
fn shutdown_cancel_command_releases_a_vetoed_request() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = app_request_shutdown_for(&service, ShutdownIntent::Quit).unwrap();

    app_cancel_shutdown_for(&service, &request.request_id).unwrap();

    assert!(service.pending_shutdown().unwrap().is_none());
}

#[test]
fn native_exit_code_is_allowed_only_for_the_committed_shutdown_intent() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = app_request_shutdown_for(&service, ShutdownIntent::Restart).unwrap();

    assert!(!app_should_allow_exit_for(&service, Some(tauri::RESTART_EXIT_CODE)).unwrap());

    app_confirm_shutdown_for(&service, &request.request_id).unwrap();
    app_commit_shutdown_for(&service, &request.request_id).unwrap();

    assert!(app_should_allow_exit_for(&service, Some(tauri::RESTART_EXIT_CODE)).unwrap());
    assert!(!app_should_allow_exit_for(&service, Some(0)).unwrap());
    assert!(!app_should_allow_exit_for(&service, None).unwrap());
}
