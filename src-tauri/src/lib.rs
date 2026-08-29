use mcode_app::{BuildLane, LaunchContext, LifecycleService};
use mcode_settings::{SettingsService, SettingsServiceError, ShutdownCheckpoint};
use serde::Serialize;
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCommandError {
    code: &'static str,
    message: String,
}

impl From<SettingsServiceError> for DesktopCommandError {
    fn from(error: SettingsServiceError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
        }
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
    .map_err(|error| DesktopCommandError {
        code: "settings_blocking_task_failed",
        message: format!("settings persistence task failed: {error}"),
    })?
    .map_err(DesktopCommandError::from)
}

#[cfg(debug_assertions)]
fn is_smoke_test() -> bool {
    std::env::var_os("MCODE_TAURI_SMOKE_TEST").is_some()
}

#[cfg(not(debug_assertions))]
fn is_smoke_test() -> bool {
    false
}

fn build_main_window(app: &tauri::App) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("MCode")
        .inner_size(1440.0, 900.0)
        .min_inner_size(800.0, 600.0)
        .resizable(true)
        .visible(!is_smoke_test());

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

    tauri::Builder::default()
        .manage(lifecycle)
        .setup(|app| {
            let settings_path = app.path().app_data_dir()?.join("mcode-data.json");
            app.manage(SettingsService::new(settings_path));
            build_main_window(app)?;
            if is_smoke_test() {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    app_handle.exit(0);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_get_launch_context,
            app_stage_shutdown_checkpoint
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MCode Tauri application");
}
