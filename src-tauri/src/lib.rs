use mcode_app::{
    BuildLane, LaunchContext, LifecycleService, LifecycleServiceError, ShutdownIntent,
};
use mcode_platform::{DesktopWindowPreferences, restorable_window_bounds};
use mcode_settings::{SettingsService, SettingsServiceError, ShutdownCheckpoint};
use serde::Serialize;
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod app_dialogs;
mod app_menu_events;
mod app_shutdown_commands;
mod clipboard_commands;
mod desktop_window;
mod export_commands;
mod filesystem_commands;
mod notification_commands;
mod os_permissions;
mod pty_commands;
mod skill_share_deep_link;
mod ssh_commands;
mod terminal_preview;
mod updater_commands;
mod windows_tray;

use app_dialogs::{pick_floating_markdown_document, pick_floating_workspace_directory};
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
use clipboard_commands::{
    ui_clipboard_read_text, ui_clipboard_write_terminal_text, ui_clipboard_write_text,
};
use desktop_window::logical_inner_size;
use desktop_window::{
    NativeWindowState, attach_placement_persistence, fit_inner_size_to_work_area,
    monitor_work_areas, primary_work_area, window_hide_to_tray_if_enabled,
    window_set_minimize_to_tray_on_close,
};
use export_commands::ui_export_html_to_pdf;
use filesystem_commands::{
    NativeFilesystemState, attach_watch_teardown, fs_cancel_list_files, fs_cancel_search_replace,
    fs_copy_path, fs_create_dir, fs_create_file, fs_delete_path, fs_path_exists, fs_read_dir,
    fs_read_file, fs_rename_path, fs_search, fs_search_replace, fs_stat, fs_unwatch_worktree,
    fs_watch_worktree, fs_write_file, fs_write_file_atomic, spawn_watch_forwarder,
};
use notification_commands::{
    ui_notification_dismiss, ui_notification_dispatch, ui_notification_get_permission_status,
    ui_notification_open_system_settings,
};
use os_permissions::{
    developer_permissions_get_status, developer_permissions_open_settings,
    developer_permissions_request, developer_permissions_test_local_network_connection,
};
pub use pty_commands::{
    NativePtyState, PtyAppliedSize, pty_ack_data_for, pty_get_size_for, pty_register_session_for,
};
use pty_commands::{
    kill_all_sessions_for_shutdown, pty_ack_data, pty_get_size, pty_has_pty, pty_kill, pty_resize,
    pty_spawn, pty_write,
};
pub use ssh_commands::{
    SshAuthDto, SshConnectResponse, SshExecResponse, SshOpenPtyResponse, SshState, SshTargetDto,
    ssh_accept_host_key_for, ssh_close_pty_for, ssh_connect_for, ssh_disconnect_for, ssh_exec_for,
    ssh_open_pty_for, ssh_resize_pty_for, ssh_write_pty_for,
};
use ssh_commands::{
    ssh_accept_host_key, ssh_close_pty, ssh_connect, ssh_disconnect, ssh_exec, ssh_open_pty,
    ssh_resize_pty, ssh_write_pty,
};
use terminal_preview::{
    terminal_preview_ack, terminal_preview_connect, terminal_preview_fit, terminal_preview_input,
    terminal_preview_unsubscribe,
};
use updater_commands::{
    NativeUpdaterState, update_check, update_download, update_get_status, update_install,
};

/// One-shot pending skill-share consume (mirrors `ui:consumePendingSkillShare`).
#[tauri::command(rename_as = "ui_consumePendingSkillShare")]
fn ui_consume_pending_skill_share(app: tauri::AppHandle) -> Option<String> {
    skill_share_deep_link::consume(&app)
}

#[tauri::command(rename_as = "ui_openSettings")]
fn ui_open_settings(app: tauri::AppHandle) -> bool {
    app_menu_events::open_settings(&app)
}

/// One-shot pending open-settings consume (mirrors `ui:consumePendingOpenSettings`).
#[tauri::command(rename_as = "ui_consumePendingOpenSettings")]
fn ui_consume_pending_open_settings(app: tauri::AppHandle) -> bool {
    app_menu_events::consume_pending_open_settings(&app)
}

#[tauri::command(rename_as = "ui_openSetupGuide")]
fn ui_open_setup_guide(app: tauri::AppHandle) -> bool {
    app_menu_events::open_setup_guide(&app)
}

#[tauri::command(rename_as = "ui_openFeatureTour")]
fn ui_open_feature_tour(app: tauri::AppHandle) -> bool {
    app_menu_events::open_feature_tour(&app)
}

#[tauri::command(rename_as = "ui_openCrashReport")]
fn ui_open_crash_report(app: tauri::AppHandle) -> bool {
    app_menu_events::open_crash_report(&app)
}

/// Floating-workspace markdown document picker (`app:pickFloatingMarkdownDocument`).
#[tauri::command(rename_as = "ui_pickFloatingMarkdownDocument")]
async fn ui_pick_floating_markdown_document(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    pick_floating_markdown_document(&app)
}

/// Floating-workspace directory picker (`app:pickFloatingWorkspaceDirectory`).
#[tauri::command(rename_as = "ui_pickFloatingWorkspaceDirectory")]
async fn ui_pick_floating_workspace_directory(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    pick_floating_workspace_directory(&app)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCommandError {
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
    Pty,
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
        if intent.eq_ignore_ascii_case("pty") {
            std::env::var_os("MCODE_TAURI_SMOKE_PTY_MARKER")?;
            std::env::var_os("MCODE_TAURI_SMOKE_PTY_ECHO")?;
            return Some(SmokeTestMode::Pty);
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
    let entrypoint = std::borrow::Cow::from(match smoke_mode {
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
        Some(SmokeTestMode::Pty) => {
            let echo = std::env::var("MCODE_TAURI_SMOKE_PTY_ECHO")
                .expect("pty smoke echo must be configured");
            let marker = std::env::var("MCODE_TAURI_SMOKE_PTY_MARKER")
                .expect("pty smoke marker must be configured");
            format!(
                "pty-smoke.html?intent=pty&echo={}&marker={}",
                encode_query_component(&echo),
                encode_query_component(&marker)
            )
        }
    });
    let entrypoint = entrypoint.as_ref();
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
            // Why: the work-area origin must be set too — an unpositioned window gets
            // cascaded by Windows default placement, hanging off the right/bottom edge.
            Some(bounds) => builder
                .position(f64::from(bounds.x), f64::from(bounds.y))
                .inner_size(f64::from(bounds.width), f64::from(bounds.height)),
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
    if install_tray {
        let fallback_app = window.app_handle().clone();
        let fallback_window_state = fallback_app.state::<NativeWindowState>().inner().clone();
        std::thread::spawn(move || {
            std::thread::sleep(WINDOWS_TRAY_FALLBACK_DELAY);
            install_windows_tray(&fallback_app, &fallback_window_state);
        });
    }
    let filesystem_state = app
        .try_state::<NativeFilesystemState>()
        .map(|state| state.inner().clone());
    if let Some(filesystem_state) = filesystem_state {
        attach_watch_teardown(&window, filesystem_state);
    }
    attach_placement_persistence(&window, native_window_state);
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
        .manage(terminal_preview::TerminalPreviewState::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            skill_share_deep_link::register(app.handle().clone());
            app_menu_events::register(app.handle());
            let pty_state = NativePtyState::new();
            let settings = SettingsService::new(settings_path(app)?);
            let (filesystem_state, watch_batches) = NativeFilesystemState::new(settings.clone());
            spawn_watch_forwarder(app.handle().clone(), watch_batches);
            app.manage(filesystem_state);
            app.manage(pty_state);
            app.manage(ssh_commands::SshState::new());
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
                Some(SmokeTestMode::Filesystem) => {
                    let marker = std::env::var_os("MCODE_TAURI_SMOKE_FILESYSTEM_MARKER")
                        .expect("filesystem smoke marker must be configured");
                    let smoke_root = std::path::PathBuf::from(
                        std::env::var("MCODE_TAURI_SMOKE_FILESYSTEM_ROOT")
                            .expect("filesystem smoke root must be configured"),
                    );
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        // Renderer readiness first, then a scoped fs_path_exists
                        // probe against the fixture root through the managed
                        // state (exercises refresh-roots + authorize + stat).
                        let renderer_passed = (0..100).any(|_| {
                            let ready = app_handle
                                .state::<NativeWindowState>()
                                .close_to_tray_revision()
                                > 0;
                            if !ready {
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            ready
                        });
                        let probe_passed = renderer_passed
                            && app_handle
                                .try_state::<NativeFilesystemState>()
                                .map(|state| state.probe_path_exists(&smoke_root).unwrap_or(false))
                                .unwrap_or(false);
                        let marker_value = if probe_passed {
                            "passed".to_owned()
                        } else {
                            "renderer-failed".to_owned()
                        };
                        if let Err(error) = std::fs::write(&marker, marker_value.as_bytes()) {
                            eprintln!("[smoke] failed to write filesystem marker: {error}");
                        }
                        app_handle.exit(i32::from(!probe_passed));
                    });
                }
                Some(SmokeTestMode::Pty) => {
                    let marker = std::env::var_os("MCODE_TAURI_SMOKE_PTY_MARKER")
                        .expect("pty smoke marker must be configured");
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        // Poll the marker file the renderer writes with its
                        // own verdict; readiness waits in the renderer. Rust
                        // never writes the marker in this mode.
                        let mut verdict: Option<String> = None;
                        for _ in 0..120 {
                            if let Ok(value) = std::fs::read_to_string(&marker) {
                                let value = value.trim();
                                if value == "passed" || value == "renderer-failed" {
                                    verdict = Some(value.to_owned());
                                    break;
                                }
                            }
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        #[cfg(debug_assertions)]
                        if let Some(pty_state) = app_handle.try_state::<NativePtyState>() {
                            eprintln!("[smoke] pty sessions live at timeout: {pty_state:?}");
                        }
                        let passed = verdict.as_deref() == Some("passed");
                        if let Some(value) = verdict {
                            eprintln!("[smoke] pty smoke renderer verdict: {value}");
                        } else {
                            eprintln!("[smoke] pty smoke marker never reached a verdict");
                        }
                        app_handle.exit(i32::from(!passed));
                    });
                }
                Some(SmokeTestMode::RestartParent) | None => {}
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            terminal_preview_ack,
            terminal_preview_connect,
            terminal_preview_fit,
            terminal_preview_input,
            terminal_preview_unsubscribe,
            ui_consume_pending_skill_share,
            ui_pick_floating_markdown_document,
            ui_pick_floating_workspace_directory,
            ui_open_settings,
            ui_consume_pending_open_settings,
            ui_open_setup_guide,
            ui_open_feature_tour,
            ui_export_html_to_pdf,
            ui_notification_dispatch,
            ui_notification_dismiss,
            ui_notification_get_permission_status,
            ui_notification_open_system_settings,
            ui_open_crash_report,
            app_get_launch_context,
            app_stage_shutdown_checkpoint,
            app_request_shutdown,
            app_acknowledge_shutdown,
            app_cancel_shutdown,
            app_confirm_and_execute_shutdown,
            window_hide_to_tray_if_enabled,
            window_set_minimize_to_tray_on_close,
            update_get_status,
            update_check,
            update_download,
            update_install,
            fs_read_dir,
            fs_read_file,
            fs_write_file,
            fs_path_exists,
            fs_stat,
            fs_create_dir,
            fs_create_file,
            fs_copy_path,
            fs_rename_path,
            fs_delete_path,
            fs_write_file_atomic,
            fs_search,
            fs_search_replace,
            fs_cancel_list_files,
            fs_cancel_search_replace,
            fs_watch_worktree,
            fs_unwatch_worktree,
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            pty_has_pty,
            pty_ack_data,
            pty_get_size,
            ssh_connect,
            ssh_accept_host_key,
            ssh_exec,
            ssh_open_pty,
            ssh_write_pty,
            ssh_resize_pty,
            ssh_close_pty,
            ssh_disconnect,
            ui_clipboard_read_text,
            ui_clipboard_write_text,
            ui_clipboard_write_terminal_text,
            developer_permissions_get_status,
            developer_permissions_request,
            developer_permissions_open_settings,
            developer_permissions_test_local_network_connection,
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
                // Every session dies before the process does: bounded
                // flush-wait then force, so no shell outlives the app.
                kill_all_sessions_for_shutdown(app_handle.state::<NativePtyState>().inner());
                app_handle.state::<NativeWindowState>().freeze_persistence();
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
