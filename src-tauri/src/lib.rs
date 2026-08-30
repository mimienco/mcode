use mcode_app::{
    BuildLane, LaunchContext, LifecycleService, LifecycleServiceError, ShutdownIntent,
};
use mcode_fs::ScopedFilesystem;
use mcode_platform::{DesktopWindowPreferences, restorable_window_bounds};
use mcode_settings::{SettingsService, SettingsServiceError, ShutdownCheckpoint};
use serde::Serialize;
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod app_shutdown_commands;
mod desktop_window;
mod filesystem_commands;
mod updater_commands;
mod windows_tray;

pub use app_shutdown_commands::{
    SHUTDOWN_RENDERER_ACK_TIMEOUT, ShutdownTimeoutAction,
    app_abort_unacknowledged_update_install_for, app_acknowledge_shutdown_for,
    app_cancel_shutdown_for, app_claim_shutdown_execution_for,
    app_confirm_and_claim_shutdown_execution_for, app_confirm_shutdown_for,
    app_force_claim_shutdown_execution_for, app_force_shutdown_after_delivery_failure_for,
    app_request_shutdown_for, app_should_allow_exit_for, shutdown_timeout_action_for,
};
use app_shutdown_commands::{
    app_acknowledge_shutdown, app_cancel_shutdown, app_confirm_and_execute_shutdown,
    app_request_shutdown, handle_exit_requested,
};
use desktop_window::logical_inner_size;
use desktop_window::{
    NativeWindowState, attach_placement_persistence, fit_inner_size_to_work_area,
    monitor_work_areas, primary_work_area, window_hide_to_tray_if_enabled,
    window_set_minimize_to_tray_on_close,
};
use filesystem_commands::{
    NativeFilesystemState, fs_path_exists, fs_read_dir, fs_read_file, fs_stat, fs_write_file_atomic,
};
use updater_commands::{
    NativeUpdaterState, update_check, update_download, update_get_status, update_install,
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
#[cfg_attr(not(debug_assertions), allow(dead_code))]
enum SmokeTestMode {
    Quit,
    RestartParent,
    RestartChild,
    Updater,
    Windowing,
    Filesystem,
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
        if intent.eq_ignore_ascii_case("updater") {
            return Some(SmokeTestMode::Updater);
        }
        if intent.eq_ignore_ascii_case("windowing") {
            std::env::var_os("MCODE_TAURI_SMOKE_WINDOWING_MARKER")?;
            return Some(SmokeTestMode::Windowing);
        }
        if intent.eq_ignore_ascii_case("filesystem") {
            std::env::var_os("MCODE_TAURI_SMOKE_FILESYSTEM_MARKER")?;
            std::env::var_os("MCODE_TAURI_SMOKE_FILESYSTEM_ROOT")?;
            return Some(SmokeTestMode::Filesystem);
        }
        Some(SmokeTestMode::Quit)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

fn expected_windowing_inner_size() -> Option<(u32, u32)> {
    let value = std::env::var("MCODE_TAURI_SMOKE_EXPECTED_INNER_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

fn settings_path(app: &tauri::App) -> tauri::Result<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("MCODE_TAURI_SETTINGS_PATH") {
        return Ok(path.into());
    }
    Ok(app.path().app_data_dir()?.join("mcode-data.json"))
}

const WINDOWS_TRAY_FALLBACK_DELAY: Duration = Duration::from_secs(12);

fn install_windows_tray(app: &tauri::AppHandle, state: &NativeWindowState) {
    match windows_tray::install(app) {
        Ok(windows_tray::TrayInstallStatus::Installed) => state.mark_tray_available(),
        Ok(windows_tray::TrayInstallStatus::InProgress) => {}
        Err(error) => eprintln!("[tray] failed to install Windows tray: {error}"),
    }
}

fn build_main_window(
    app: &tauri::App,
    smoke_mode: Option<SmokeTestMode>,
    preferences: DesktopWindowPreferences,
    native_window_state: NativeWindowState,
) -> tauri::Result<()> {
    let entrypoint = match smoke_mode {
        Some(SmokeTestMode::RestartParent) => "lifecycle-smoke.html?intent=restart".to_owned(),
        Some(SmokeTestMode::Quit) => "lifecycle-smoke.html?intent=quit".to_owned(),
        Some(SmokeTestMode::RestartChild) | None => "index.html".to_owned(),
        Some(SmokeTestMode::Updater) => "lifecycle-smoke.html?intent=quit&updater=true".to_owned(),
        Some(SmokeTestMode::Windowing) => {
            "lifecycle-smoke.html?intent=quit&windowing=true".to_owned()
        }
        Some(SmokeTestMode::Filesystem) => {
            let root = std::env::var("MCODE_TAURI_SMOKE_FILESYSTEM_ROOT")
                .expect("filesystem smoke root must be configured");
            format!(
                "lifecycle-smoke.html?intent=quit&filesystem=true&root={}",
                encode_query_component(&root)
            )
        }
    };
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(entrypoint.into()))
        .title("MCode")
        .min_inner_size(600.0, 400.0)
        .resizable(true)
        .visible(smoke_mode.is_none() || smoke_mode == Some(SmokeTestMode::Windowing));

    let monitor_bounds = monitor_work_areas(app).unwrap_or_else(|error| {
        eprintln!("[window] failed to enumerate monitors: {error}");
        Vec::new()
    });
    let restored_bounds = restorable_window_bounds(preferences.bounds, &monitor_bounds);
    let mut default_work_area = None;
    if let Some(bounds) = restored_bounds {
        builder = builder
            .position(f64::from(bounds.x), f64::from(bounds.y))
            .inner_size(f64::from(bounds.width), f64::from(bounds.height));
    } else {
        if preferences.bounds.is_some() {
            eprintln!("[window] discarded persisted off-screen or near-minimum bounds");
        }
        default_work_area = primary_work_area(app).unwrap_or_else(|error| {
            eprintln!("[window] failed to inspect primary monitor: {error}");
            None
        });
        builder = match default_work_area {
            Some(bounds) => builder.inner_size(f64::from(bounds.width), f64::from(bounds.height)),
            None => builder.inner_size(1200.0, 800.0),
        };
    }

    let install_tray = smoke_mode.is_none() || smoke_mode == Some(SmokeTestMode::Windowing);
    if install_tray {
        let page_load_window_state = native_window_state.clone();
        builder = builder.on_page_load(move |window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                install_windows_tray(window.app_handle(), &page_load_window_state);
            }
        });
    }

    #[cfg(debug_assertions)]
    if let Some(data_directory) = std::env::var_os("MCODE_TAURI_WEBVIEW_DATA_DIR") {
        builder = builder.data_directory(data_directory.into());
    }

    let window = builder.build()?;
    if install_tray {
        install_windows_tray(window.app_handle(), &native_window_state);
    }
    if let Some(work_area) = default_work_area {
        fit_inner_size_to_work_area(&window, work_area)?;
    }
    attach_placement_persistence(&window, native_window_state);
    if install_tray {
        let fallback_app = window.app_handle().clone();
        let fallback_window_state = fallback_app.state::<NativeWindowState>().inner().clone();
        std::thread::spawn(move || {
            std::thread::sleep(WINDOWS_TRAY_FALLBACK_DELAY);
            install_windows_tray(&fallback_app, &fallback_window_state);
        });
    }
    if preferences.maximized {
        window.maximize()?;
    }
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
        .manage(NativeUpdaterState::from_compile_time())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let settings = SettingsService::new(settings_path(app)?);
            let scoped_filesystem = ScopedFilesystem::new();
            app.manage(NativeFilesystemState::new(
                scoped_filesystem,
                settings.clone(),
            ));
            let preferences = settings
                .desktop_window_preferences()
                .unwrap_or_else(|error| {
                    eprintln!(
                        "[window] failed to read persisted desktop preferences; using safe defaults: {error}"
                    );
                    DesktopWindowPreferences::default()
                });
            let native_window_state = NativeWindowState::new(settings.clone(), preferences);
            app.manage(settings);
            app.manage(native_window_state.clone());
            let smoke_mode = smoke_test_mode();
            if smoke_mode == Some(SmokeTestMode::RestartParent) {
                let marker = std::env::var_os("MCODE_TAURI_SMOKE_RESTART_MARKER")
                    .expect("restart smoke marker must be configured");
                std::fs::write(marker, b"restart-requested")?;
            }
            if smoke_mode != Some(SmokeTestMode::RestartChild) {
                build_main_window(app, smoke_mode, preferences, native_window_state)?;
            }
            match smoke_mode {
                Some(SmokeTestMode::Quit) | Some(SmokeTestMode::Updater) => {
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(3));
                        eprintln!(
                            "[smoke] requesting app-wide quit through the renderer acknowledgement path"
                        );
                        app_handle.exit(0);
                    });
                }
                Some(SmokeTestMode::Windowing) => {
                    let marker = std::env::var_os("MCODE_TAURI_SMOKE_WINDOWING_MARKER")
                        .expect("windowing smoke marker must be configured");
                    let expected_inner_size = expected_windowing_inner_size();
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        let renderer_ready = (0..300).any(|_| {
                            let ready = app_handle
                                .state::<NativeWindowState>()
                                .close_to_tray_revision()
                                > 0;
                            if !ready {
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            ready
                        });
                        if renderer_ready {
                            if let Some(window) = app_handle.get_webview_window("main")
                                && let Err(error) = window.close()
                            {
                                eprintln!("[smoke] failed to send native window close: {error}");
                            }
                        } else {
                            eprintln!("[smoke] renderer did not signal windowing readiness");
                        }
                        std::thread::sleep(Duration::from_secs(2));
                        let window = app_handle.get_webview_window("main");
                        let hidden = window
                            .as_ref()
                            .and_then(|window| window.is_visible().ok())
                            .is_some_and(|visible| !visible);
                        let actual_inner_size = window
                            .as_ref()
                            .and_then(|window| logical_inner_size(window).ok());
                        let restored_size = expected_inner_size
                            .is_none_or(|expected| actual_inner_size == Some(expected));
                        let tray_available = app_handle
                            .state::<NativeWindowState>()
                            .tray_available();
                        let result = hidden && tray_available && restored_size;
                        if !result {
                            eprintln!(
                                "[smoke] windowing evidence failed: hidden={hidden}, tray_available={tray_available}, expected_inner_size={expected_inner_size:?}, actual_inner_size={actual_inner_size:?}"
                            );
                        }
                        let marker_value: &[u8] =
                            if result { b"hidden" } else { b"not-hidden" };
                        if let Err(error) = std::fs::write(marker, marker_value) {
                            eprintln!("[smoke] failed to write windowing marker: {error}");
                        }
                        app_handle.exit(0);
                    });
                }
                Some(SmokeTestMode::Filesystem) => {
                    let marker = std::env::var_os("MCODE_TAURI_SMOKE_FILESYSTEM_MARKER")
                        .expect("filesystem smoke marker must be configured");
                    let root = std::env::var_os("MCODE_TAURI_SMOKE_FILESYSTEM_ROOT")
                        .expect("filesystem smoke root must be configured");
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        let mut renderer_revision = 0;
                        let renderer_passed = (0..100).any(|_| {
                            renderer_revision = app_handle
                                .state::<NativeWindowState>()
                                .close_to_tray_revision();
                            let ready = renderer_revision > 0;
                            if !ready {
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            ready
                        });
                        let renderer_error = std::fs::read_to_string(
                            std::path::Path::new(&root)
                                .join(".mcode-filesystem-smoke-error.txt"),
                        )
                        .ok();
                        let marker_value = if let Some(error) = renderer_error {
                            format!("renderer-error:{error}")
                        } else if renderer_passed {
                            "passed".to_owned()
                        } else {
                            "renderer-failed".to_owned()
                        };
                        if let Err(error) = std::fs::write(marker, marker_value.as_bytes()) {
                            eprintln!("[smoke] failed to write filesystem marker: {error}");
                        }
                        app_handle.exit(i32::from(!renderer_passed));
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
            app_confirm_and_execute_shutdown,
            window_hide_to_tray_if_enabled,
            window_set_minimize_to_tray_on_close,
            fs_read_dir,
            fs_read_file,
            fs_write_file_atomic,
            fs_stat,
            fs_path_exists,
            update_get_status,
            update_check,
            update_download,
            update_install
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
                app_handle.state::<NativeWindowState>().freeze_persistence();
                return;
            }
            api.prevent_exit();
            handle_exit_requested(app_handle, code);
        }
    });
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(encoded, "%{byte:02X}").expect("writing to a string is infallible");
        }
    }
    encoded
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

    #[test]
    fn smoke_query_values_are_percent_encoded() {
        assert_eq!(
            encode_query_component(r"C:\Users\Milan M\repo"),
            "C%3A%5CUsers%5CMilan%20M%5Crepo"
        );
    }
}
