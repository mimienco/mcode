use mcode_updater::{
    AvailableUpdate, ReleaseChannel, UpdateOperation, UpdateService, UpdateServiceError,
    UpdateStatus,
};
use serde_json::json;

fn candidate() -> AvailableUpdate {
    AvailableUpdate {
        channel: ReleaseChannel::Hourly,
        current_version: "1.4.178".to_owned(),
        version: "1.4.179-hourly.202608301530".to_owned(),
        body: Some("Native updater slice".to_owned()),
        published_at: Some("2026-08-30T15:30:00Z".to_owned()),
    }
}

#[test]
fn check_download_and_apply_are_explicit_ordered_transitions() {
    let service = UpdateService::new();

    service.begin_check(ReleaseChannel::Hourly, true).unwrap();
    assert_eq!(
        service.status().unwrap(),
        UpdateStatus::Checking {
            channel: ReleaseChannel::Hourly,
            user_initiated: true,
        }
    );

    service.complete_check(Some(candidate())).unwrap();
    service.begin_download().unwrap();
    service.record_download_progress(25, Some(100)).unwrap();
    service.record_download_progress(25, Some(100)).unwrap();
    assert_eq!(
        service.status().unwrap(),
        UpdateStatus::Downloading {
            channel: ReleaseChannel::Hourly,
            version: candidate().version,
            downloaded_bytes: 50,
            total_bytes: Some(100),
            percent: Some(50),
        }
    );

    service.complete_download().unwrap();
    service.begin_install().unwrap();
    assert_eq!(
        service.status().unwrap(),
        UpdateStatus::Installing {
            channel: ReleaseChannel::Hourly,
            version: candidate().version,
        }
    );
}

#[test]
fn download_and_install_are_refused_without_verified_prior_state() {
    let service = UpdateService::new();

    assert_eq!(
        service.begin_download().unwrap_err(),
        UpdateServiceError::NoAvailableUpdate
    );
    assert_eq!(
        service.begin_install().unwrap_err(),
        UpdateServiceError::NoDownloadedUpdate
    );
}

#[test]
fn concurrent_operations_are_refused_without_corrupting_the_active_check() {
    let service = UpdateService::new();
    service.begin_check(ReleaseChannel::Stable, false).unwrap();

    assert_eq!(
        service.begin_check(ReleaseChannel::Rc, true).unwrap_err(),
        UpdateServiceError::OperationInProgress(UpdateOperation::Check)
    );
    assert_eq!(
        service.status().unwrap(),
        UpdateStatus::Checking {
            channel: ReleaseChannel::Stable,
            user_initiated: false,
        }
    );
}

#[test]
fn failed_install_keeps_verified_bytes_retryable_but_never_reports_success() {
    let service = UpdateService::new();
    service.begin_check(ReleaseChannel::Hourly, true).unwrap();
    service.complete_check(Some(candidate())).unwrap();
    service.begin_download().unwrap();
    service.complete_download().unwrap();
    service.begin_install().unwrap();

    service
        .fail(
            UpdateOperation::Install,
            "native_install_failed",
            "installer refused the package",
            true,
        )
        .unwrap();

    assert!(matches!(
        service.status().unwrap(),
        UpdateStatus::Error {
            operation: UpdateOperation::Install,
            retryable: true,
            ..
        }
    ));
    service.begin_install().unwrap();
}

#[test]
fn status_contract_is_tagged_and_camel_cased_for_the_renderer() {
    let service = UpdateService::new();
    service.begin_check(ReleaseChannel::Daily, true).unwrap();

    assert_eq!(
        serde_json::to_value(service.status().unwrap()).unwrap(),
        json!({
            "state": "checking",
            "channel": "daily",
            "userInitiated": true
        })
    );
}
