use mcode_app::{
    BuildLane, LaunchContext, LifecycleService, LifecycleServiceError, ShutdownIntent,
    ShutdownRequest,
};

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

#[test]
fn shutdown_stays_pending_until_the_renderer_confirms_the_exact_request() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);

    let request = service.request_shutdown(ShutdownIntent::Restart).unwrap();

    assert_eq!(
        request,
        ShutdownRequest {
            version: 1,
            request_id: "1".to_owned(),
            intent: ShutdownIntent::Restart,
        }
    );
    assert_eq!(service.pending_shutdown().unwrap(), Some(request.clone()));
    assert_eq!(
        service.confirm_shutdown(&request.request_id).unwrap(),
        request
    );
}

#[test]
fn a_new_shutdown_request_invalidates_a_stale_renderer_acknowledgement() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let stale = service.request_shutdown(ShutdownIntent::Quit).unwrap();
    let current = service.request_shutdown(ShutdownIntent::Restart).unwrap();

    assert_eq!(
        service.confirm_shutdown(&stale.request_id),
        Err(LifecycleServiceError::StaleShutdownRequest)
    );
    assert_eq!(service.pending_shutdown().unwrap(), Some(current.clone()));
    assert_eq!(
        service.confirm_shutdown(&current.request_id).unwrap(),
        current
    );
}

#[test]
fn native_exit_is_authorized_only_after_the_confirmed_request_is_committed() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Quit).unwrap();

    assert!(!service.is_shutdown_executing(ShutdownIntent::Quit).unwrap());
    assert_eq!(
        service.claim_shutdown_execution(&request.request_id),
        Err(LifecycleServiceError::ShutdownNotConfirmed)
    );

    service.confirm_shutdown(&request.request_id).unwrap();
    assert_eq!(
        service
            .claim_shutdown_execution(&request.request_id)
            .unwrap(),
        Some(request.clone())
    );
    assert_eq!(
        service
            .claim_shutdown_execution(&request.request_id)
            .unwrap(),
        None
    );
    assert!(service.is_shutdown_executing(ShutdownIntent::Quit).unwrap());
    assert!(
        !service
            .is_shutdown_executing(ShutdownIntent::Restart)
            .unwrap()
    );
}

#[test]
fn a_committed_shutdown_cannot_be_replaced_before_native_exit() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Restart).unwrap();
    service.confirm_shutdown(&request.request_id).unwrap();
    service
        .claim_shutdown_execution(&request.request_id)
        .unwrap();

    assert_eq!(
        service.request_shutdown(ShutdownIntent::Quit),
        Err(LifecycleServiceError::ShutdownAlreadyExecuting)
    );
    assert!(
        service
            .is_shutdown_executing(ShutdownIntent::Restart)
            .unwrap()
    );
}

#[test]
fn a_renderer_veto_cancels_only_its_pending_shutdown_request() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Quit).unwrap();

    service.cancel_shutdown(&request.request_id).unwrap();

    assert_eq!(service.pending_shutdown().unwrap(), None);
    assert_eq!(
        service.confirm_shutdown(&request.request_id),
        Err(LifecycleServiceError::NoPendingShutdown)
    );
    assert_eq!(
        service.cancel_shutdown(&request.request_id),
        Err(LifecycleServiceError::NoPendingShutdown)
    );
}

#[test]
fn a_failed_native_execute_can_cancel_a_confirmed_request_but_not_an_executing_one() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let failed = service.request_shutdown(ShutdownIntent::Restart).unwrap();
    service.confirm_shutdown(&failed.request_id).unwrap();

    service.cancel_shutdown(&failed.request_id).unwrap();

    let executing = service.request_shutdown(ShutdownIntent::Quit).unwrap();
    service.confirm_shutdown(&executing.request_id).unwrap();
    service
        .claim_shutdown_execution(&executing.request_id)
        .unwrap();
    assert_eq!(
        service.cancel_shutdown(&executing.request_id),
        Err(LifecycleServiceError::ShutdownAlreadyExecuting)
    );
}

#[test]
fn a_renderer_timeout_claims_pending_shutdown_execution_only_once() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Quit).unwrap();

    assert_eq!(
        service
            .force_claim_shutdown_execution(&request.request_id)
            .unwrap(),
        Some(request.clone())
    );
    assert_eq!(
        service
            .force_claim_shutdown_execution(&request.request_id)
            .unwrap(),
        None
    );
    assert!(service.is_shutdown_executing(ShutdownIntent::Quit).unwrap());
}

#[test]
fn renderer_receipt_disarms_the_timeout_while_the_user_decides() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Quit).unwrap();

    assert_eq!(
        service.acknowledge_shutdown(&request.request_id).unwrap(),
        request
    );
    assert_eq!(
        service
            .force_claim_shutdown_execution(&request.request_id)
            .unwrap(),
        None
    );
    assert!(!service.is_shutdown_executing(ShutdownIntent::Quit).unwrap());

    service.cancel_shutdown(&request.request_id).unwrap();
    assert_eq!(service.pending_shutdown().unwrap(), None);
}

#[test]
fn a_confirmed_request_can_be_force_claimed_if_execution_is_interrupted() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Restart).unwrap();
    service.acknowledge_shutdown(&request.request_id).unwrap();
    service.confirm_shutdown(&request.request_id).unwrap();

    assert_eq!(
        service
            .force_claim_shutdown_execution(&request.request_id)
            .unwrap(),
        Some(request.clone())
    );
    assert!(
        service
            .is_shutdown_executing(ShutdownIntent::Restart)
            .unwrap()
    );
}

#[test]
fn a_canceled_shutdown_cannot_be_reclaimed_by_its_timeout() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Restart).unwrap();
    service.cancel_shutdown(&request.request_id).unwrap();

    assert_eq!(
        service.force_claim_shutdown_execution(&request.request_id),
        Err(LifecycleServiceError::NoPendingShutdown)
    );
    assert!(
        !service
            .is_shutdown_executing(ShutdownIntent::Restart)
            .unwrap()
    );
}

#[test]
fn a_confirmed_request_must_be_canceled_before_another_request_can_start() {
    let service = LifecycleService::new("1.4.178", BuildLane::Development);
    let request = service.request_shutdown(ShutdownIntent::Restart).unwrap();
    service.confirm_shutdown(&request.request_id).unwrap();

    assert_eq!(
        service.request_shutdown(ShutdownIntent::Quit),
        Err(LifecycleServiceError::ShutdownAlreadyConfirmed)
    );
}

#[test]
fn shutdown_request_fixture_matches_the_rust_contract() {
    let request: ShutdownRequest =
        serde_json::from_str(include_str!("fixtures/shutdown-request-v1.json")).unwrap();

    assert_eq!(request.version, 1);
    assert_eq!(request.request_id, "42");
    assert_eq!(request.intent, ShutdownIntent::Restart);
}

#[test]
fn failed_update_install_execution_can_return_to_idle_without_weakening_other_requests() {
    let service = LifecycleService::new("1.4.178", BuildLane::Production);
    let request = service
        .request_shutdown(ShutdownIntent::UpdateInstall)
        .unwrap();
    service.acknowledge_shutdown(&request.request_id).unwrap();
    service.confirm_shutdown(&request.request_id).unwrap();
    service
        .claim_shutdown_execution(&request.request_id)
        .unwrap();

    service
        .abort_update_install_execution(&request.request_id)
        .unwrap();

    assert!(service.pending_shutdown().unwrap().is_none());
    assert!(
        !service
            .is_shutdown_executing(ShutdownIntent::UpdateInstall)
            .unwrap()
    );
    assert!(service.request_shutdown(ShutdownIntent::Quit).is_ok());
}

#[test]
fn executing_quit_or_restart_cannot_use_update_install_recovery() {
    for intent in [ShutdownIntent::Quit, ShutdownIntent::Restart] {
        let service = LifecycleService::new("1.4.178", BuildLane::Production);
        let request = service.request_shutdown(intent).unwrap();
        service.confirm_shutdown(&request.request_id).unwrap();
        service
            .claim_shutdown_execution(&request.request_id)
            .unwrap();

        assert!(
            service
                .abort_update_install_execution(&request.request_id)
                .is_err()
        );
        assert!(service.is_shutdown_executing(intent).unwrap());
    }
}

#[test]
fn only_an_unacknowledged_update_install_can_be_aborted_by_its_timeout() {
    let pending_service = LifecycleService::new("1.4.178", BuildLane::Production);
    let pending = pending_service
        .request_shutdown(ShutdownIntent::UpdateInstall)
        .unwrap();
    assert!(
        pending_service
            .abort_unacknowledged_update_install(&pending.request_id)
            .unwrap()
    );
    assert!(pending_service.pending_shutdown().unwrap().is_none());

    let acknowledged_service = LifecycleService::new("1.4.178", BuildLane::Production);
    let acknowledged = acknowledged_service
        .request_shutdown(ShutdownIntent::UpdateInstall)
        .unwrap();
    acknowledged_service
        .acknowledge_shutdown(&acknowledged.request_id)
        .unwrap();
    assert!(
        !acknowledged_service
            .abort_unacknowledged_update_install(&acknowledged.request_id)
            .unwrap()
    );
    assert_eq!(
        acknowledged_service.pending_shutdown().unwrap(),
        Some(acknowledged)
    );
}
