//! Thin Tauri command wrapper over the settings crossgrade pipeline in
//! `mcode-settings::crossgrade_staging`. Not compiled until the orchestrator
//! wires `mod crossgrade_commands;` into `src-tauri/src/lib.rs`.
//!
//! Surface decision (D-012): crossgrading is an explicit, user-triggered
//! command from the settings UI — never an automatic startup hook. The
//! migration doc requires discovery to stay read-only until a VERIFIED
//! migration completes; auto-running the pipeline at startup would write
//! into the app-data dir before any user-visible verification.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use mcode_settings::crossgrade_staging::{self, CrossgradeError, StagedCrossgrade};

use crate::DesktopCommandError;

#[allow(dead_code)] // reserved for the status response operation echo
const STATUS_OPERATION: &str = "settings_crossgrade_status";
const MIGRATE_OPERATION: &str = "settings_crossgrade_migrate";

// ─── DTOs (camelCase on the wire) ───────────────────────────────────────────

/// Read-only discovery snapshot for the settings UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossgradeStatusDto {
    /// An Electron userData dir with at least one marker file was found.
    pub discovered: bool,
    /// The user's own Electron app dir — safe to expose to their renderer.
    pub electron_root: Option<String>,
    /// The primary settings document (mcode-data.json) exists there.
    pub has_settings_source: bool,
    /// Always false today: a plan is built inside one migrate call and is
    /// not persisted across calls (stage/verify/commit is resumable by plan
    /// hash instead).
    pub plan_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossgradeResultDto {
    pub plan_version: u32,
    pub migrated_keys: u32,
    pub host_keys_copied: bool,
    pub telemetry_preserved: bool,
    pub staging_discarded: bool,
}

// ─── Errors ─────────────────────────────────────────────────────────────────

fn crossgrade_command_error(
    error: CrossgradeError,
    operation: &'static str,
) -> DesktopCommandError {
    let user_message = match error.code() {
        "crossgrade_source_missing" => {
            "MCode could not find the original MCode data folder to migrate from."
        }
        "crossgrade_source_corrupt" => {
            "The original MCode data folder holds unreadable data. It was not modified."
        }
        "crossgrade_verify_failed" => {
            "MCode could not verify the migrated settings. Nothing was changed; try again."
        }
        "crossgrade_plan_hash_mismatch" | "crossgrade_plan_version_unsupported" => {
            "A previous migration attempt conflicts with this one. Nothing was changed; try again."
        }
        _ => "MCode could not complete the settings migration. Try again.",
    };
    DesktopCommandError::new(
        error.code(),
        operation,
        error.to_string(),
        user_message,
        None,
        None,
    )
}

fn app_data_error(operation: &'static str) -> DesktopCommandError {
    DesktopCommandError::new(
        "crossgrade_target_unavailable",
        operation,
        "app data directory unavailable".into(),
        "MCode could not locate its own data folder. Try again.",
        None,
        None,
    )
}

// ─── Commands ───────────────────────────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn settings_crossgrade_status() -> CrossgradeStatusDto {
    let location = mcode_settings::crossgrade_discovery::discover_electron_data_locations()
        .into_iter()
        .next();
    match location {
        Some(location) => CrossgradeStatusDto {
            discovered: true,
            electron_root: Some(location.path.to_string_lossy().into_owned()),
            has_settings_source: location.has_primary_settings(),
            plan_ready: false,
        },
        None => CrossgradeStatusDto {
            discovered: false,
            electron_root: None,
            has_settings_source: false,
            plan_ready: false,
        },
    }
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn settings_crossgrade_migrate(
    app: AppHandle,
) -> Result<CrossgradeResultDto, DesktopCommandError> {
    // Debug builds honor MCODE_TAURI_SETTINGS_PATH (lib.rs:342-347 discipline):
    // the migration target is the PARENT of that settings file, not the real
    // app-data dir — the smoke runs redirect it to a fixture COPY and must
    // never touch the real profile.
    #[cfg(debug_assertions)]
    let target_dir = match std::env::var_os("MCODE_TAURI_SETTINGS_PATH") {
        Some(path) => std::path::PathBuf::from(path)
            .parent()
            .map(std::path::Path::to_path_buf)
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| app_data_error(MIGRATE_OPERATION))?,
        None => app
            .path()
            .app_data_dir()
            .map_err(|_| app_data_error(MIGRATE_OPERATION))?,
    };
    #[cfg(not(debug_assertions))]
    let target_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| app_data_error(MIGRATE_OPERATION))?;

    let source_root = mcode_settings::crossgrade_discovery::discover_electron_data_locations()
        .into_iter()
        .find(mcode_settings::crossgrade_discovery::ElectronDataLocation::has_primary_settings)
        .map(|location| location.path)
        .ok_or_else(|| {
            crossgrade_command_error(
                CrossgradeError::SourceMissing {
                    path: target_dir.join(crossgrade_staging::ELECTRON_SETTINGS_FILE),
                },
                MIGRATE_OPERATION,
            )
        })?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default();

    let plan = crossgrade_staging::build_plan(&source_root, &target_dir, now_ms)
        .map_err(|error| crossgrade_command_error(error, MIGRATE_OPERATION))?;
    let staged: StagedCrossgrade = crossgrade_staging::stage(&plan, &target_dir, now_ms)
        .map_err(|error| crossgrade_command_error(error, MIGRATE_OPERATION))?;
    let report = crossgrade_staging::verify(&staged)
        .map_err(|error| crossgrade_command_error(error, MIGRATE_OPERATION))?;
    crossgrade_staging::commit(&staged, &target_dir)
        .map_err(|error| crossgrade_command_error(error, MIGRATE_OPERATION))?;
    let staging_discarded = crossgrade_staging::discard(&staged)
        .map_err(|error| crossgrade_command_error(error, MIGRATE_OPERATION))?;

    Ok(CrossgradeResultDto {
        plan_version: report.plan_version,
        migrated_keys: report.settings_checked,
        host_keys_copied: report.host_keys_byte_identical,
        telemetry_preserved: report.telemetry_preserved,
        staging_discarded,
    })
}
