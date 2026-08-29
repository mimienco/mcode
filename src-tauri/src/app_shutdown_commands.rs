use mcode_app::{LifecycleService, LifecycleServiceError, ShutdownIntent, ShutdownRequest};
use tauri::{AppHandle, Emitter, Manager};

use crate::DesktopCommandError;

pub const APP_SHUTDOWN_REQUESTED_EVENT: &str = "mcode://app/shutdown-requested";

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

pub fn app_confirm_shutdown_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<ShutdownRequest, LifecycleServiceError> {
    service.confirm_shutdown(request_id)
}

pub fn app_commit_shutdown_for(
    service: &LifecycleService,
    request_id: &str,
) -> Result<ShutdownRequest, LifecycleServiceError> {
    service.commit_shutdown(request_id)
}

pub fn app_should_allow_exit_for(
    service: &LifecycleService,
    exit_code: Option<i32>,
) -> Result<bool, LifecycleServiceError> {
    let intent = match exit_code {
        Some(tauri::RESTART_EXIT_CODE) => ShutdownIntent::Restart,
        Some(_) => ShutdownIntent::Quit,
        None => return Ok(false),
    };
    service.is_shutdown_executing(intent)
}

fn emit_shutdown_request(
    app: &AppHandle,
    service: &LifecycleService,
    intent: ShutdownIntent,
) -> Result<ShutdownRequest, DesktopCommandError> {
    let main_window = app
        .get_webview_window("main")
        .ok_or_else(|| DesktopCommandError {
            code: "main_window_unavailable",
            message: "the main window is unavailable for shutdown confirmation".to_owned(),
        })?;
    let request = app_request_shutdown_for(service, intent)?;
    let emit_result = main_window.emit(APP_SHUTDOWN_REQUESTED_EVENT, request.clone());
    if let Err(error) = emit_result {
        let _ = app_cancel_shutdown_for(service, &request.request_id);
        return Err(DesktopCommandError {
            code: "shutdown_request_emit_failed",
            message: format!("failed to deliver shutdown request: {error}"),
        });
    }
    Ok(request)
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
pub(crate) fn app_cancel_shutdown(
    service: tauri::State<'_, LifecycleService>,
    request_id: String,
) -> Result<(), DesktopCommandError> {
    app_cancel_shutdown_for(&service, &request_id).map_err(DesktopCommandError::from)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn app_confirm_shutdown(
    service: tauri::State<'_, LifecycleService>,
    request_id: String,
) -> Result<ShutdownRequest, DesktopCommandError> {
    app_confirm_shutdown_for(&service, &request_id).map_err(DesktopCommandError::from)
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn app_execute_shutdown(
    app: AppHandle,
    service: tauri::State<'_, LifecycleService>,
    request_id: String,
) -> Result<(), DesktopCommandError> {
    let request = app_commit_shutdown_for(&service, &request_id)?;
    match request.intent {
        ShutdownIntent::Quit => app.exit(0),
        ShutdownIntent::Restart => app.request_restart(),
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
    }
}
