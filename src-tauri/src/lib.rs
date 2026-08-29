use mcode_app::{BuildLane, LaunchContext, LifecycleService};
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindowBuilder};

pub fn app_get_launch_context_for(service: &LifecycleService) -> LaunchContext {
    service.launch_context()
}

#[tauri::command]
fn app_get_launch_context(service: tauri::State<'_, LifecycleService>) -> LaunchContext {
    app_get_launch_context_for(&service)
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
        .invoke_handler(tauri::generate_handler![app_get_launch_context])
        .run(tauri::generate_context!())
        .expect("failed to run MCode Tauri application");
}
