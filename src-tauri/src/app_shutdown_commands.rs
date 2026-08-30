use mcode_app::{LifecycleService, LifecycleServiceError, ShutdownIntent, ShutdownRequest};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::DesktopCommandError;

pub const APP_SHUTDOWN_REQUESTED_EVENT: &str = "mcode://app/shutdown-requested";
pub const SHUTDOWN_RENDERER_ACK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownTimeoutAction {
    Force,
    Abort,
}

pub fn shutdown_timeout_action_for(intent: ShutdownIntent) -> ShutdownTimeoutAction {
    match intent {
        ShutdownIntent::Quit | ShutdownIntent::Restart => ShutdownTimeoutAction::Force,
        ShutdownIntent::UpdateInstall => ShutdownTimeoutAction::Abort,
    }
}

pub fn app_request_shutdown_for(
    service: &LifecycleService,
    intent: ShutdownIntent,
) -> Result<ShutdownRequest, LifecycleServiceError> {
    service.request_shutdown(intent)
}

pub fn app_cancel_shutdown_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<(), LifecycleServiceError> {
    service.cancel_shutdown(request_id)
}

pub fn app_acknowledge_shutdown_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<ShutdownRequest, LifecycleServiceError> {
    service.acknowledge_shutdown(request_id)
}

pub fn app_confirm_shutdown_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<ShutdownRequest, LifecycleServiceError> {
    service.confirm_shutdown(request_id)
}

pub fn app_claim_shutdown_execution_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
    service.claim_shutdown_execution(request_id)
}

pub fn app_force_claim_shutdown_execution_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
    service.force_claim_shutdown_execution(request_id)
}

pub fn app_force_shutdown_after_delivery_failure_for(
    service: &LifecycleService,
    intent: ShutdownIntent,
) -> Result<ShutdownRequest, LifecycleServiceError> {
    let request = app_request_shutdown_for(service, intent)?;
    app_force_claim_shutdown_execution_for(service, &request.request_id)?
        .ok_or(LifecycleServiceError::ShutdownAlreadyExecuting)
}

pub fn app_abort_unacknowledged_update_install_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<bool, LifecycleServiceError> {
    service.abort_unacknowledged_update_install(request_id)
}

pub fn app_confirm_and_claim_shutdown_execution_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
    app_confirm_shutdown_for(service, request_id)?;
    app_claim_shutdown_execution_for(service, request_id)
}

pub fn app_should_allow_exit_for(
    service: &LifecycleService,
    exit_code: Option<i32>,
) -> Result<bool, LifecycleServiceError> {
    match exit_code {
        Some(tauri::RESTART_EXIT_CODE) => Ok(service
            .is_shutdown_executing(ShutdownIntent::Restart)?
            || service.is_shutdown_executing(ShutdownIntent::UpdateInstall)?),
        Some(_) => Ok(service.is_shutdown_executing(ShutdownIntent::Quit)?
            || service.is_shutdown_executing(ShutdownIntent::UpdateInstall)?),
        None => Ok(false),
    }
}

pub(crate) fn emit_shutdown_request(
    app: &AppHandle,
    service: &LifecycleService,
    intent: ShutdownIntent,
) -> Result<ShutdownRequest, DesktopCommandError> {
    let main_window = app.get_webview_window("main").ok_or_else(|| {
        DesktopCommandError::new(
            "main_window_unavailable",
            "app_request_shutdown",
            "the main window is unavailable for shutdown confirmation".to_owned(),
            "MCode could not contact its main window. Try again.",
            None,
            Some(intent),
        )
    })?;
    let request = app_request_shutdown_for(service, intent).map_err(|error| {
        DesktopCommandError::lifecycle(error, "app_request_shutdown", None, Some(intent))
    })?;
    let emit_result = main_window.emit(APP_SHUTDOWN_REQUESTED_EVENT, request.clone());
    if let Err(error) = emit_result {
        let _ = app_cancel_shutdown_for(service, &request.request_id);
        return Err(DesktopCommandError::new(
            "shutdown_request_emit_failed",
            "app_request_shutdown",
            format!("failed to deliver shutdown request: {error}"),
            "MCode could not contact its main window. Try again.",
            Some(&request.request_id),
            Some(request.intent),
        ));
    }
    arm_shutdown_timeout(app.clone(), request.clone());
    Ok(request)
}

fn execute_shutdown_request(
    app: &AppHandle,
    request: &ShutdownRequest,
) -> Result<(), DesktopCommandError> {
    match request.intent {
        ShutdownIntent::Quit => app.exit(0),
        ShutdownIntent::Restart => app.request_restart(),
        ShutdownIntent::UpdateInstall => {
            let result = crate::updater_commands::execute_downloaded_update(app);
            if result.is_err() {
                let lifecycle = app.state::<LifecycleService>();
                if let Err(error) = lifecycle.abort_update_install_execution(&request.request_id) {
                    eprintln!(
                        "[updater] failed to recover lifecycle after update install failure: {error}"
                    );
                }
            }
            return result;
        }
    }
    Ok(())
}

fn arm_shutdown_timeout(app: AppHandle, request: ShutdownRequest) {
    std::thread::spawn(move || {
        std::thread::sleep(SHUTDOWN_RENDERER_ACK_TIMEOUT);
        let service = app.state::<LifecycleService>();
        if shutdown_timeout_action_for(request.intent) == ShutdownTimeoutAction::Abort {
            match app_abort_unacknowledged_update_install_for(&service, &request.request_id) {
                Ok(true) => {
                    eprintln!(
                        "[updater] renderer did not acknowledge update install {}; installation aborted",
                        request.request_id
                    );
                    if let Err(error) = app.emit(
                        crate::updater_commands::UPDATE_INSTALL_ABORTED_EVENT,
                        &request,
                    ) {
                        eprintln!(
                            "[updater] failed to notify renderer about aborted install {}: {error}",
                            request.request_id
                        );
                    }
                }
                Ok(false)
                | Err(LifecycleServiceError::NoPendingShutdown)
                | Err(LifecycleServiceError::StaleShutdownRequest) => {}
                Err(error) => eprintln!(
                    "[updater] failed to abort unacknowledged update install {}: {error}",
                    request.request_id
                ),
            }
            return;
        }
        match app_force_claim_shutdown_execution_for(&service, &request.request_id) {
            Ok(Some(claimed)) => {
                eprintln!(
                    "[lifecycle] renderer did not acknowledge shutdown request {}; forcing {:?}",
                    claimed.request_id, claimed.intent
                );
                if let Err(error) = execute_shutdown_request(&app, &claimed) {
                    eprintln!("[lifecycle] forced shutdown execution failed: {error:?}");
                }
            }
            Ok(None)
            | Err(LifecycleServiceError::NoPendingShutdown)
            | Err(LifecycleServiceError::StaleShutdownRequest) => {}
            Err(error) => {
                eprintln!(
                    "[lifecycle] failed to apply shutdown timeout for request {}: {error}",
                    request.request_id
                );
            }
        }
    });
}

#[tauri::command]
pub(crate) fn app_request_shutdown(
    app: AppHandle,
    service: tauri::State<'_, LifecycleService>,
    intent: ShutdownIntent,
) -> Result<ShutdownRequest, DesktopCommandError> {
    emit_shutdown_request(&app, &service, intent)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn app_acknowledge_shutdown(
    service: tauri::State<'_, LifecycleService>,
    request_id: String,
) -> Result<(), DesktopCommandError> {
    let request = app_acknowledge_shutdown_for(&service, &request_id).map_err(|error| {
        DesktopCommandError::lifecycle(error, "app_acknowledge_shutdown", Some(&request_id), None)
    })?;
    eprintln!(
        "[lifecycle] renderer received shutdown request {} ({:?})",
        request.request_id, request.intent
    );
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn app_cancel_shutdown(
    service: tauri::State<'_, LifecycleService>,
    request_id: String,
) -> Result<(), DesktopCommandError> {
    app_cancel_shutdown_for(&service, &request_id).map_err(|error| {
        DesktopCommandError::lifecycle(error, "app_cancel_shutdown", Some(&request_id), None)
    })
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn app_confirm_and_execute_shutdown(
    app: AppHandle,
    service: tauri::State<'_, LifecycleService>,
    request_id: String,
) -> Result<(), DesktopCommandError> {
    let request =
        app_confirm_and_claim_shutdown_execution_for(&service, &request_id).map_err(|error| {
            DesktopCommandError::lifecycle(
                error,
                "app_confirm_and_execute_shutdown",
                Some(&request_id),
                None,
            )
        })?;
    if let Some(request) = request {
        eprintln!(
            "[lifecycle] renderer acknowledged shutdown request {} ({:?})",
            request.request_id, request.intent
        );
        execute_shutdown_request(&app, &request)?;
    }
    Ok(())
}

pub(crate) fn handle_exit_requested(app: &AppHandle, exit_code: Option<i32>) {
    let service = app.state::<LifecycleService>();
    match app_should_allow_exit_for(&service, exit_code) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("[lifecycle] failed to inspect shutdown state: {error}");
            return;
        }
    }
    if exit_code == Some(tauri::RESTART_EXIT_CODE) {
        eprintln!(
            "[lifecycle] unconfirmed restart reached the run loop; Tauri cannot cancel restart exits"
        );
        return;
    }
    if let Err(error) = emit_shutdown_request(app, &service, ShutdownIntent::Quit) {
        eprintln!("[lifecycle] failed to request renderer shutdown acknowledgement: {error:?}");
        match app_force_shutdown_after_delivery_failure_for(&service, ShutdownIntent::Quit) {
            Ok(request) => {
                eprintln!(
                    "[lifecycle] renderer delivery failed for shutdown request {}; forcing {:?}",
                    request.request_id, request.intent
                );
                if let Err(error) = execute_shutdown_request(app, &request) {
                    eprintln!("[lifecycle] forced delivery-failure shutdown failed: {error:?}");
                }
            }
            Err(force_error) => {
                eprintln!(
                    "[lifecycle] failed to recover from renderer shutdown delivery failure: {force_error}"
                );
            }
        }
    }
}
