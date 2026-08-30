use mcode_app::{BuildLane, LifecycleService, ShutdownIntent};
use mcode_desktop::{
    SHUTDOWN_RENDERER_ACK_TIMEOUT, ShutdownTimeoutAction,
    app_abort_unacknowledged_update_install_for, app_acknowledge_shutdown_for,
    app_cancel_shutdown_for, app_claim_shutdown_execution_for,
    app_confirm_and_claim_shutdown_execution_for, app_confirm_shutdown_for,
    app_force_claim_shutdown_execution_for, app_force_shutdown_after_delivery_failure_for,
    app_request_shutdown_for, app_should_allow_exit_for, shutdown_timeout_action_for,
};
use serde_json::json;
use std::time::Duration;

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
        app_claim_shutdown_execution_for(&service, &request.request_id).unwrap(),
        Some(request)
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
    app_claim_shutdown_execution_for(&service, &request.request_id).unwrap();

    assert!(app_should_allow_exit_for(&service, Some(tauri::RESTART_EXIT_CODE)).unwrap());
    assert!(!app_should_allow_exit_for(&service, Some(0)).unwrap());
    assert!(!app_should_allow_exit_for(&service, None).unwrap());
}

#[test]
fn renderer_ack_timeout_matches_the_existing_electron_quit_bound() {
    assert_eq!(SHUTDOWN_RENDERER_ACK_TIMEOUT, Duration::from_secs(10));
}

#[test]
fn renderer_ack_timeout_claims_the_exact_pending_request_once() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = app_request_shutdown_for(&service, ShutdownIntent::Quit).unwrap();

    assert_eq!(
        app_force_claim_shutdown_execution_for(&service, &request.request_id).unwrap(),
        Some(request.clone())
    );
    assert_eq!(
        app_force_claim_shutdown_execution_for(&service, &request.request_id).unwrap(),
        None
    );
}

#[test]
fn renderer_receipt_ack_disarms_the_force_timeout_without_confirming_exit() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = app_request_shutdown_for(&service, ShutdownIntent::Quit).unwrap();

    app_acknowledge_shutdown_for(&service, &request.request_id).unwrap();

    assert_eq!(
        app_force_claim_shutdown_execution_for(&service, &request.request_id).unwrap(),
        None
    );
    assert!(!app_should_allow_exit_for(&service, Some(0)).unwrap());
}

#[test]
fn confirm_and_claim_is_one_backend_transition_before_native_execution() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = app_request_shutdown_for(&service, ShutdownIntent::Restart).unwrap();
    app_acknowledge_shutdown_for(&service, &request.request_id).unwrap();

    assert_eq!(
        app_confirm_and_claim_shutdown_execution_for(&service, &request.request_id).unwrap(),
        Some(request.clone())
    );
    assert!(app_should_allow_exit_for(&service, Some(tauri::RESTART_EXIT_CODE)).unwrap());
}

#[test]
fn native_delivery_failure_forces_a_fresh_executing_quit() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let abandoned = app_request_shutdown_for(&service, ShutdownIntent::Restart).unwrap();
    app_acknowledge_shutdown_for(&service, &abandoned.request_id).unwrap();

    let forced =
        app_force_shutdown_after_delivery_failure_for(&service, ShutdownIntent::Quit).unwrap();

    assert_ne!(forced.request_id, abandoned.request_id);
    assert_eq!(forced.intent, ShutdownIntent::Quit);
    assert!(app_should_allow_exit_for(&service, Some(0)).unwrap());
}

#[test]
fn update_install_uses_the_versioned_shutdown_contract() {
    let service = LifecycleService::new("1.4.178", BuildLane::Production);

    let request = app_request_shutdown_for(&service, ShutdownIntent::UpdateInstall).unwrap();

    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({
            "version": 1,
            "requestId": "1",
            "intent": "update_install"
        })
    );
}

#[test]
fn update_install_never_runs_on_the_unacknowledged_timeout_path() {
    assert_eq!(
        shutdown_timeout_action_for(ShutdownIntent::UpdateInstall),
        ShutdownTimeoutAction::Abort
    );
    assert_eq!(
        shutdown_timeout_action_for(ShutdownIntent::Quit),
        ShutdownTimeoutAction::Force
    );
    assert_eq!(
        shutdown_timeout_action_for(ShutdownIntent::Restart),
        ShutdownTimeoutAction::Force
    );
}

#[test]
fn renderer_receipt_disarms_the_update_install_abort_timeout() {
    let service = LifecycleService::new("1.4.178", BuildLane::Production);
    let request = app_request_shutdown_for(&service, ShutdownIntent::UpdateInstall).unwrap();
    app_acknowledge_shutdown_for(&service, &request.request_id).unwrap();

    assert!(!app_abort_unacknowledged_update_install_for(&service, &request.request_id).unwrap());
    assert_eq!(service.pending_shutdown().unwrap(), Some(request));
}

#[test]
fn executing_update_install_allows_the_installers_platform_exit_or_relaunch() {
    let service = LifecycleService::new("1.4.178", BuildLane::Production);
    let request = app_request_shutdown_for(&service, ShutdownIntent::UpdateInstall).unwrap();
    app_confirm_shutdown_for(&service, &request.request_id).unwrap();
    app_claim_shutdown_execution_for(&service, &request.request_id).unwrap();

    assert!(app_should_allow_exit_for(&service, Some(0)).unwrap());
    assert!(app_should_allow_exit_for(&service, Some(tauri::RESTART_EXIT_CODE)).unwrap());
}
