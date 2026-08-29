use mcode_app::{
    BuildLane, LaunchContext, LifecycleService, LifecycleServiceError, ShutdownIntent,
};
use mcode_settings::{SettingsService, SettingsServiceError, ShutdownCheckpoint};
use serde::Serialize;
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod app_shutdown_commands;

pub use app_shutdown_commands::{
    SHUTDOWN_RENDERER_ACK_TIMEOUT, app_acknowledge_shutdown_for, app_cancel_shutdown_for,
    app_claim_shutdown_execution_for, app_confirm_and_claim_shutdown_execution_for,
    app_confirm_shutdown_for, app_force_claim_shutdown_execution_for,
    app_force_shutdown_after_delivery_failure_for, app_request_shutdown_for,
    app_should_allow_exit_for,
};
use app_shutdown_commands::{
    app_acknowledge_shutdown, app_cancel_shutdown, app_confirm_and_execute_shutdown,
    app_request_shutdown, handle_exit_requested,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCommandError {
    code: &'static str,
    context: DesktopCommandErrorContext,
    message: String,
    user_message: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCommandErrorContext {
    operation: &'static str,
    request_id: Option<String>,
    intent: Option<ShutdownIntent>,
}

impl DesktopCommandError {
    fn new(
        code: &'static str,
        operation: &'static str,
        message: String,
        user_message: &'static str,
        request_id: Option<&str>,
        intent: Option<ShutdownIntent>,
    ) -> Self {
        Self {
            code,
            context: DesktopCommandErrorContext {
                operation,
                request_id: request_id.map(str::to_owned),
                intent,
            },
            message,
            user_message,
        }
    }

    fn lifecycle(
        error: LifecycleServiceError,
        operation: &'static str,
        request_id: Option<&str>,
        intent: Option<ShutdownIntent>,
    ) -> Self {
        Self::new(
            error.code(),
            operation,
            error.to_string(),
            "MCode could not complete the shutdown request. Try again.",
            request_id,
            intent,
        )
    }

    fn settings(error: SettingsServiceError, operation: &'static str) -> Self {
        Self::new(
            error.code(),
            operation,
            error.to_string(),
            "MCode could not save your session. Try again.",
            None,
            None,
        )
    }
}

pub fn app_get_launch_context_for(service: &LifecycleService) -> LaunchContext {
    service.launch_context()
}

#[tauri::command]
fn app_get_launch_context(service: tauri::State<'_, LifecycleService>) -> LaunchContext {
    app_get_launch_context_for(&service)
}

pub fn app_stage_shutdown_checkpoint_for(
    service: &SettingsService,
    checkpoint: ShutdownCheckpoint,
) -> Result<(), SettingsServiceError> {
    service.stage_shutdown_checkpoint(checkpoint)
}

#[tauri::command]
async fn app_stage_shutdown_checkpoint(
    service: tauri::State<'_, SettingsService>,
    checkpoint: ShutdownCheckpoint,
) -> Result<(), DesktopCommandError> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_stage_shutdown_checkpoint_for(&service, checkpoint)
    })
    .await
    .map_err(|error| {
        DesktopCommandError::new(
            "settings_blocking_task_failed",
            "app_stage_shutdown_checkpoint",
            format!("settings persistence task failed: {error}"),
            "MCode could not save your session. Try again.",
            None,
            None,
        )
    })?
    .map_err(|error| DesktopCommandError::settings(error, "app_stage_shutdown_checkpoint"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SmokeTestMode {
    Quit,
    RestartParent,
    RestartChild,
}

fn smoke_test_mode() -> Option<SmokeTestMode> {
    #[cfg(debug_assertions)]
    {
        let intent = std::env::var("MCODE_TAURI_SMOKE_TEST").ok()?;
        if intent.eq_ignore_ascii_case("restart") {
            let marker = std::env::var_os("MCODE_TAURI_SMOKE_RESTART_MARKER")?;
            return Some(if std::path::Path::new(&marker).exists() {
                SmokeTestMode::RestartChild
            } else {
                SmokeTestMode::RestartParent
            });
        }
        Some(SmokeTestMode::Quit)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

fn settings_path(app: &tauri::App) -> tauri::Result<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("MCODE_TAURI_SETTINGS_PATH") {
        return Ok(path.into());
    }
    Ok(app.path().app_data_dir()?.join("mcode-data.json"))
}

fn build_main_window(app: &tauri::App, smoke_mode: Option<SmokeTestMode>) -> tauri::Result<()> {
    let entrypoint = match smoke_mode {
        Some(SmokeTestMode::RestartParent) => "lifecycle-smoke.html?intent=restart",
        Some(SmokeTestMode::Quit) => "lifecycle-smoke.html?intent=quit",
        Some(SmokeTestMode::RestartChild) | None => "index.html",
    };
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(entrypoint.into()))
        .title("MCode")
        .inner_size(1440.0, 900.0)
        .min_inner_size(800.0, 600.0)
        .resizable(true)
        .visible(smoke_mode.is_none());

    #[cfg(debug_assertions)]
    if let Some(data_directory) = std::env::var_os("MCODE_TAURI_WEBVIEW_DATA_DIR") {
        builder = builder.data_directory(data_directory.into());
    }

    builder.build()?;
    Ok(())
}

pub fn run() {
    let build_lane = if cfg!(debug_assertions) {
        BuildLane::Development
    } else {
        BuildLane::Production
    };
    let lifecycle = LifecycleService::new(env!("CARGO_PKG_VERSION"), build_lane);

    let app = tauri::Builder::default()
        .manage(lifecycle)
        .setup(|app| {
            app.manage(SettingsService::new(settings_path(app)?));
            let smoke_mode = smoke_test_mode();
            if smoke_mode == Some(SmokeTestMode::RestartParent) {
                let marker = std::env::var_os("MCODE_TAURI_SMOKE_RESTART_MARKER")
                    .expect("restart smoke marker must be configured");
                std::fs::write(marker, b"restart-requested")?;
            }
            if smoke_mode != Some(SmokeTestMode::RestartChild) {
                build_main_window(app, smoke_mode)?;
            }
            match smoke_mode {
                Some(SmokeTestMode::Quit) => {
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(3));
                        eprintln!(
                            "[smoke] requesting app-wide quit through the renderer acknowledgement path"
                        );
                        app_handle.exit(0);
                    });
                }
                Some(SmokeTestMode::RestartChild) => {
                    let marker = std::env::var_os("MCODE_TAURI_SMOKE_RESTART_MARKER")
                        .expect("restart smoke marker must be configured");
                    std::fs::write(marker, b"relaunched")?;
                    eprintln!("[smoke] restarted child launched");
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(1));
                        let lifecycle = app_handle.state::<LifecycleService>();
                        if let Ok(request) = lifecycle.request_shutdown(ShutdownIntent::Quit) {
                            let _ = lifecycle.confirm_shutdown(&request.request_id);
                            let _ = lifecycle.claim_shutdown_execution(&request.request_id);
                        }
                        app_handle.exit(0);
                    });
                }
                Some(SmokeTestMode::RestartParent) | None => {}
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_get_launch_context,
            app_stage_shutdown_checkpoint,
            app_request_shutdown,
            app_acknowledge_shutdown,
            app_cancel_shutdown,
            app_confirm_and_execute_shutdown
        ])
        .build(tauri::generate_context!())
        .expect("failed to build MCode Tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
            let lifecycle = app_handle.state::<LifecycleService>();
            let allow_exit = app_should_allow_exit_for(&lifecycle, code).unwrap_or_else(|error| {
                eprintln!("[lifecycle] failed to inspect shutdown state: {error}");
                false
            });
            if allow_exit {
                return;
            }
            api.prevent_exit();
            handle_exit_requested(app_handle, code);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn desktop_command_errors_include_code_context_and_safe_user_message() {
        let error = DesktopCommandError::lifecycle(
            LifecycleServiceError::StaleShutdownRequest,
            "app_confirm_and_execute_shutdown",
            Some("42"),
            Some(ShutdownIntent::Restart),
        );

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "stale_shutdown_request",
                "context": {
                    "operation": "app_confirm_and_execute_shutdown",
                    "requestId": "42",
                    "intent": "restart"
                },
                "message": "shutdown request does not match the pending request",
                "userMessage": "MCode could not complete the shutdown request. Try again."
            })
        );
    }
}
