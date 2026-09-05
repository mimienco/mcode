use mcode_app::{LifecycleService, ShutdownRequest};
use mcode_updater::{
    AvailableUpdate, ReleaseChannel, ReleaseFeedConfiguration, UpdateOperation, UpdateService,
    UpdateServiceError, UpdateStatus,
};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::DesktopCommandError;
use crate::app_shutdown_commands::emit_shutdown_request;

pub const UPDATE_STATUS_EVENT: &str = "mcode://updater/status";
pub const UPDATE_INSTALL_ABORTED_EVENT: &str = "mcode://updater/install-aborted";

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckRequest {
    pub channel: ReleaseChannel,
    #[serde(default)]
    pub user_initiated: bool,
}

#[derive(Default)]
struct DownloadedUpdate {
    update: Option<Update>,
    bytes: Option<Vec<u8>>,
}

pub struct NativeUpdaterState {
    service: UpdateService,
    configuration: Result<ReleaseFeedConfiguration, String>,
    downloaded: Mutex<DownloadedUpdate>,
}

impl NativeUpdaterState {
    pub fn from_compile_time() -> Self {
        Self::new(compiled_release_feed_configuration())
    }

    fn new(configuration: Result<ReleaseFeedConfiguration, String>) -> Self {
        Self {
            service: UpdateService::new(),
            configuration,
            downloaded: Mutex::new(DownloadedUpdate::default()),
        }
    }

    fn configuration(
        &self,
        operation: &'static str,
    ) -> Result<&ReleaseFeedConfiguration, DesktopCommandError> {
        self.configuration.as_ref().map_err(|error| {
            DesktopCommandError::new(
                "updater_not_configured",
                operation,
                error.clone(),
                "Updates are not configured for this build.",
                None,
                None,
            )
        })
    }

    fn lock_downloaded(
        &self,
        operation: &'static str,
    ) -> Result<std::sync::MutexGuard<'_, DownloadedUpdate>, DesktopCommandError> {
        self.downloaded.lock().map_err(|_| {
            DesktopCommandError::new(
                "update_state_unavailable",
                operation,
                "downloaded update state is unavailable".to_owned(),
                "MCode could not access the downloaded update. Try again.",
                None,
                None,
            )
        })
    }
}

fn compiled_release_feed_configuration() -> Result<ReleaseFeedConfiguration, String> {
    ReleaseFeedConfiguration::new(
        option_env!("MCODE_TAURI_UPDATER_PUBLIC_KEY").unwrap_or_default(),
        [
            (
                ReleaseChannel::Stable,
                option_env!("MCODE_TAURI_UPDATER_STABLE_ENDPOINT").unwrap_or_default(),
            ),
            (
                ReleaseChannel::Rc,
                option_env!("MCODE_TAURI_UPDATER_RC_ENDPOINT").unwrap_or_default(),
            ),
            (
                ReleaseChannel::Hourly,
                option_env!("MCODE_TAURI_UPDATER_HOURLY_ENDPOINT").unwrap_or_default(),
            ),
            (
                ReleaseChannel::Daily,
                option_env!("MCODE_TAURI_UPDATER_DAILY_ENDPOINT").unwrap_or_default(),
            ),
            (
                ReleaseChannel::Adhoc,
                option_env!("MCODE_TAURI_UPDATER_ADHOC_ENDPOINT").unwrap_or_default(),
            ),
        ],
    )
    .map_err(|error| error.to_string())
}

fn service_error(error: UpdateServiceError, operation: &'static str) -> DesktopCommandError {
    DesktopCommandError::new(
        error.code(),
        operation,
        error.to_string(),
        "MCode could not complete the update request. Try again.",
        None,
        None,
    )
}

fn native_error(
    operation: UpdateOperation,
    operation_name: &'static str,
    code: &'static str,
    error: impl std::fmt::Display,
) -> DesktopCommandError {
    DesktopCommandError::new(
        code,
        operation_name,
        error.to_string(),
        match operation {
            UpdateOperation::Check => "MCode could not check for updates. Try again.",
            UpdateOperation::Download => "MCode could not download the update. Try again.",
            UpdateOperation::Install => {
                "MCode could not install the update. The application remains open."
            }
        },
        None,
        None,
    )
}

fn emit_status(app: &AppHandle, service: &UpdateService) {
    match service.status() {
        Ok(status) => {
            if let Err(error) = app.emit(UPDATE_STATUS_EVENT, status) {
                eprintln!("[updater] failed to emit updater status: {error}");
            }
        }
        Err(error) => eprintln!("[updater] failed to read updater status: {error}"),
    }
}

fn fail_operation(
    app: &AppHandle,
    state: &NativeUpdaterState,
    operation: UpdateOperation,
    code: &'static str,
    error: &impl std::fmt::Display,
) {
    if let Err(state_error) = state.service.fail(operation, code, error.to_string(), true) {
        eprintln!("[updater] failed to record {operation:?} failure: {state_error}");
    }
    emit_status(app, &state.service);
}

#[tauri::command]
pub(crate) fn update_get_status(
    state: tauri::State<'_, NativeUpdaterState>,
) -> Result<UpdateStatus, DesktopCommandError> {
    state
        .service
        .status()
        .map_err(|error| service_error(error, "update_get_status"))
}

#[tauri::command]
pub(crate) async fn update_check(
    app: AppHandle,
    state: tauri::State<'_, NativeUpdaterState>,
    request: UpdateCheckRequest,
) -> Result<UpdateStatus, DesktopCommandError> {
    state
        .service
        .begin_check(request.channel, request.user_initiated)
        .map_err(|error| service_error(error, "update_check"))?;
    {
        let mut downloaded = state.lock_downloaded("update_check")?;
        downloaded.update = None;
        downloaded.bytes = None;
    }
    emit_status(&app, &state.service);
    let configuration = match state.configuration("update_check") {
        Ok(configuration) => configuration.clone(),
        Err(error) => {
            #[cfg(debug_assertions)]
            if let Some(path) = std::env::var_os("MCODE_TAURI_UPDATER_SMOKE_RESULT")
                && let Err(write_error) = std::fs::write(path, error.code.as_bytes())
            {
                eprintln!("[smoke] failed to write updater smoke result: {write_error}");
            }
            fail_operation(
                &app,
                &state,
                UpdateOperation::Check,
                "updater_not_configured",
                &error.message,
            );
            return Err(error);
        }
    };

    let updater = app
        .updater_builder()
        .pubkey(configuration.public_key())
        .endpoints(vec![configuration.endpoint(request.channel).clone()])
        .and_then(|builder| builder.build());
    let updater = match updater {
        Ok(updater) => updater,
        Err(error) => {
            fail_operation(
                &app,
                &state,
                UpdateOperation::Check,
                "native_update_configuration_failed",
                &error,
            );
            return Err(native_error(
                UpdateOperation::Check,
                "update_check",
                "native_update_configuration_failed",
                error,
            ));
        }
    };
    let update = match updater.check().await {
        Ok(update) => update,
        Err(error) => {
            fail_operation(
                &app,
                &state,
                UpdateOperation::Check,
                "native_update_check_failed",
                &error,
            );
            return Err(native_error(
                UpdateOperation::Check,
                "update_check",
                "native_update_check_failed",
                error,
            ));
        }
    };

    let candidate = update.as_ref().map(|update| AvailableUpdate {
        channel: request.channel,
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        body: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    });
    state
        .service
        .complete_check(candidate)
        .map_err(|error| service_error(error, "update_check"))?;
    state.lock_downloaded("update_check")?.update = update;
    emit_status(&app, &state.service);
    state
        .service
        .status()
        .map_err(|error| service_error(error, "update_check"))
}

#[tauri::command]
pub(crate) async fn update_download(
    app: AppHandle,
    state: tauri::State<'_, NativeUpdaterState>,
) -> Result<UpdateStatus, DesktopCommandError> {
    let update = state
        .lock_downloaded("update_download")?
        .update
        .clone()
        .ok_or_else(|| service_error(UpdateServiceError::NoAvailableUpdate, "update_download"))?;
    state
        .service
        .begin_download()
        .map_err(|error| service_error(error, "update_download"))?;
    emit_status(&app, &state.service);

    let progress_app = app.clone();
    let progress_state = state.inner();
    let bytes = update
        .download(
            move |chunk_bytes, total_bytes| {
                if progress_state
                    .service
                    .record_download_progress(chunk_bytes, total_bytes)
                    .is_ok()
                {
                    emit_status(&progress_app, &progress_state.service);
                }
            },
            || {},
        )
        .await;
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(error) => {
            fail_operation(
                &app,
                &state,
                UpdateOperation::Download,
                "native_update_download_failed",
                &error,
            );
            return Err(native_error(
                UpdateOperation::Download,
                "update_download",
                "native_update_download_failed",
                error,
            ));
        }
    };

    state.lock_downloaded("update_download")?.bytes = Some(bytes);
    state
        .service
        .complete_download()
        .map_err(|error| service_error(error, "update_download"))?;
    emit_status(&app, &state.service);
    state
        .service
        .status()
        .map_err(|error| service_error(error, "update_download"))
}

#[tauri::command]
pub(crate) fn update_install(
    app: AppHandle,
    lifecycle: tauri::State<'_, LifecycleService>,
    state: tauri::State<'_, NativeUpdaterState>,
) -> Result<ShutdownRequest, DesktopCommandError> {
    match state
        .service
        .status()
        .map_err(|error| service_error(error, "update_install"))?
    {
        UpdateStatus::Downloaded { .. }
        | UpdateStatus::Error {
            operation: UpdateOperation::Install,
            retryable: true,
            ..
        } => emit_shutdown_request(&app, &lifecycle, mcode_app::ShutdownIntent::UpdateInstall),
        _ => Err(service_error(
            UpdateServiceError::NoDownloadedUpdate,
            "update_install",
        )),
    }
}

pub(crate) fn execute_downloaded_update(app: &AppHandle) -> Result<(), DesktopCommandError> {
    let state = app.state::<NativeUpdaterState>();
    let (update, bytes) = {
        let mut downloaded = state.lock_downloaded("update_install")?;
        let update = downloaded.update.clone().ok_or_else(|| {
            service_error(UpdateServiceError::NoDownloadedUpdate, "update_install")
        })?;
        let bytes = downloaded.bytes.take().ok_or_else(|| {
            service_error(UpdateServiceError::NoDownloadedUpdate, "update_install")
        })?;
        (update, bytes)
    };
    if let Err(error) = state.service.begin_install() {
        if let Ok(mut downloaded) = state.lock_downloaded("update_install") {
            downloaded.bytes = Some(bytes);
        }
        return Err(service_error(error, "update_install"));
    }
    emit_status(app, &state.service);

    if let Err(error) = update.install(&bytes) {
        if let Ok(mut downloaded) = state.lock_downloaded("update_install") {
            downloaded.bytes = Some(bytes);
        }
        fail_operation(
            app,
            &state,
            UpdateOperation::Install,
            "native_update_install_failed",
            &error,
        );
        return Err(native_error(
            UpdateOperation::Install,
            "update_install",
            "native_update_install_failed",
            error,
        ));
    }

    // The Windows updater exits the process after launching the installer. On macOS and Linux,
    // install returns after replacing the bundle, so the confirmed lifecycle performs the relaunch.
    // Release the single-instance mutex before the relaunch (see
    // execute_shutdown_request) so the relaunched process can acquire it.
    crate::release_single_instance_lock();
    app.request_restart();
    Ok(())
}
