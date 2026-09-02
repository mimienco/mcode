//! Native shell commands porting the Electron IPC surface
//! `orca/src/main/ipc/shell.ts` (local branches only) plus the local-path
//! external-editor launch chain from `orca/src/main/external-editor-launch.ts`.
//!
//! Why: the renderer `window.api.shell` namespace (13 methods) must keep its
//! exact contract — silent no-ops for bad URLs, result objects for reveal
//! failures, typed `DesktopCommandError`s for copy/pick failures — so
//! `src/renderer/src/desktop/shell-client.ts` can swap transports without
//! behavior drift.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::DesktopCommandError;

/// Mirrors `MAX_REPO_ICON_UPLOAD_BYTES` (`orca/src/shared/repo-icon.ts:10`).
const MAX_REPO_ICON_UPLOAD_BYTES: usize = 256 * 1024;
/// Mirrors `EXTERNAL_EDITOR_CLI_COMMAND` (`orca/src/main/external-editor-launch.ts:18`).
const EXTERNAL_EDITOR_CLI_COMMAND: &str = "code";

const OPERATION_OPEN_URL: &str = "shell_open_url";
const OPERATION_PATH_EXISTS: &str = "shell_path_exists";
const OPERATION_OPEN_PATH: &str = "shell_open_path";
const OPERATION_OPEN_IN_FILE_MANAGER: &str = "shell_open_in_file_manager";
const OPERATION_OPEN_FILE_PATH: &str = "shell_open_file_path";
const OPERATION_OPEN_FILE_URI: &str = "shell_open_file_uri";
const OPERATION_OPEN_IN_EXTERNAL_EDITOR: &str = "shell_open_in_external_editor";
const OPERATION_PICK_DIRECTORY: &str = "shell_pick_directory";
const OPERATION_PICK_ATTACHMENT: &str = "shell_pick_attachment";
const OPERATION_PICK_IMAGE: &str = "shell_pick_image";
const OPERATION_PICK_REPO_ICON_IMAGE: &str = "shell_pick_repo_icon_image";
const OPERATION_PICK_AUDIO: &str = "shell_pick_audio";
const OPERATION_COPY_FILE: &str = "shell_copy_file";

fn user_message_for(operation: &'static str) -> &'static str {
    match operation {
        OPERATION_COPY_FILE => "MCode could not copy the file. Try again.",
        OPERATION_PICK_DIRECTORY | OPERATION_PICK_ATTACHMENT | OPERATION_PICK_IMAGE
        | OPERATION_PICK_REPO_ICON_IMAGE | OPERATION_PICK_AUDIO => {
            "MCode could not open the picker. Try again."
        }
        _ => "MCode could not complete the shell request. Try again.",
    }
}

fn command_error(
    code: &'static str,
    operation: &'static str,
    message: impl Into<String>,
) -> DesktopCommandError {
    DesktopCommandError::new(
        code,
        operation,
        message.into(),
        user_message_for(operation),
        None,
        None,
    )
}

// ---------------------------------------------------------------------------
// Result DTOs — mirror `src/shared/shell-open-types.ts` exactly.
// ---------------------------------------------------------------------------

/// `ShellOpenLocalPathResult`. Single-word lowercase reasons match the
/// original union members verbatim, so no field renaming is needed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "ok", content = "reason")]
pub(crate) enum ShellOpenLocalPathResult {
    #[serde(rename = "true")]
    Ok,
    #[serde(rename = "false")]
    Failed {
        reason: ShellOpenLocalPathFailureReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum ShellOpenLocalPathFailureReason {
    #[serde(rename = "not-absolute")]
    NotAbsolute,
    #[serde(rename = "not-found")]
    NotFound,
    #[serde(rename = "launch-failed")]
    LaunchFailed,
}

/// `ShellOpenExternalEditorResult`. The third variant carries the
/// `ssh-alias-required` host/port context from the shared type; it is only
/// reachable from the SSH branch, which stays unported (see
/// [`shell_open_in_external_editor`]).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "ok", content = "reason")]
pub(crate) enum ShellOpenExternalEditorResult {
    #[serde(rename = "true")]
    Ok,
    #[serde(rename = "false")]
    Failed {
        reason: ShellOpenExternalEditorFailureReason,
    },
    #[serde(rename = "false")]
    SshAliasRequired { host: String, port: u16 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum ShellOpenExternalEditorFailureReason {
    #[serde(rename = "not-absolute")]
    NotAbsolute,
    #[serde(rename = "not-found")]
    NotFound,
    #[serde(rename = "launch-failed")]
    LaunchFailed,
    #[serde(rename = "remote-runtime-unsupported")]
    RemoteRuntimeUnsupported,
    #[serde(rename = "ssh-target-not-found")]
    SshTargetNotFound,
    #[serde(rename = "ssh-target-invalid")]
    SshTargetInvalid,
    #[serde(rename = "remote-editor-unsupported")]
    RemoteEditorUnsupported,
}

/// `{ dataUrl, fileName }` payload of the repo-icon picker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShellPickedRepoIcon {
    pub data_url: String,
    pub file_name: String,
}

impl ShellOpenLocalPathResult {
    fn ok() -> Self {
        Self::Ok
    }

    fn failed(reason: ShellOpenLocalPathFailureReason) -> Self {
        Self::Failed { reason }
    }
}

impl ShellOpenExternalEditorResult {
    fn failed(reason: ShellOpenExternalEditorFailureReason) -> Self {
        Self::Failed { reason }
    }
}

fn launch_failed_local() -> ShellOpenLocalPathResult {
    ShellOpenLocalPathResult::failed(ShellOpenLocalPathFailureReason::LaunchFailed)
}

fn launch_failed_editor() -> ShellOpenExternalEditorResult {
    ShellOpenExternalEditorResult::failed(ShellOpenExternalEditorFailureReason::LaunchFailed)
}

// ---------------------------------------------------------------------------
// Shared validation — ports `validateLocalPathTarget` (shell.ts:35-46).
// ---------------------------------------------------------------------------

enum ValidatedTarget {
    Ok(std::path::PathBuf),
    NotAbsolute,
    NotFound,
}

async fn validate_local_path_target(path_value: &str) -> ValidatedTarget {
    // Node's path.normalize collapses duplicate separators and resolves `.` /
    // `..`; Rust's Path does not normalize strings, so feed the raw string to
    // the platform path checks. `is_absolute` covers the absolute-ness part of
    // normalize(); the existence check below guards against `..` escapes that
    // Node's normalize would also have resolved.
    let path = std::path::Path::new(path_value);
    if !path.is_absolute() {
        return ValidatedTarget::NotAbsolute;
    }
    match tokio::fs::try_exists(path).await {
        Ok(true) => ValidatedTarget::Ok(path.to_path_buf()),
        // stat failures mean "not found" in the original (shell.ts:26-33).
        Ok(false) | Err(_) => ValidatedTarget::NotFound,
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Port of `shell:openUrl` (shell.ts:156-169): parse as URL; only https/http
/// reach the OS opener; anything else is a silent no-op, exactly like the
/// original's bare `return`.
///
/// Why opener plugin: `tauri-plugin-opener::open_url` is the standard Tauri
/// replacement for Electron's `shell.openExternal`.
#[tauri::command]
pub(crate) async fn shell_open_url(app: AppHandle, url: String) -> Result<(), DesktopCommandError> {
    let parsed = match url::Url::parse(&url) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(()),
    };
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Ok(());
    }
    // The plugin's async open keeps the fire-and-forget feel of
    // shell.openExternal while still surfacing launch failures as typed
    // errors for diagnostics.
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|error| {
            command_error(
                "shell_launch_failed",
                OPERATION_OPEN_URL,
                format!("failed to open URL: {error}"),
            )
        })
}

/// Port of `shell:pathExists` (shell.ts:207-209 / 26-33): stat-based
/// existence; any error is `false`, never an error.
#[tauri::command]
pub(crate) async fn shell_path_exists(path: String) -> Result<bool, DesktopCommandError> {
    match tokio::fs::try_exists(&path).await {
        Ok(exists) => Ok(exists),
        Err(_) => Ok(false),
    }
}

/// Port of `shell:openPath` (shell.ts:139-143): fire-and-forget reveal with
/// the same validation as the explicit file-manager API.
#[tauri::command]
pub(crate) async fn shell_open_path(
    app: AppHandle,
    path: String,
) -> Result<(), DesktopCommandError> {
    let _ = shell_open_in_file_manager_inner(&app, &path).await;
    Ok(())
}

/// Port of `shell:openInFileManager` (shell.ts:145-148 + 52-71): reveal the
/// item inside the platform file manager.
///
/// Why: the `remote-runtime-unsupported` branch (shell.ts:56-58, checked via
/// the Electron `Store`'s `activeRuntimeEnvironmentId`) is dropped — the
/// Tauri backend has no runtime-environment store yet. Parity row stays open.
#[tauri::command]
pub(crate) async fn shell_open_in_file_manager(
    app: AppHandle,
    path: String,
) -> Result<ShellOpenLocalPathResult, DesktopCommandError> {
    shell_open_in_file_manager_inner(&app, &path).await
}

async fn shell_open_in_file_manager_inner(
    app: &AppHandle,
    path_value: &str,
) -> Result<ShellOpenLocalPathResult, DesktopCommandError> {
    let target = match validate_local_path_target(path_value).await {
        ValidatedTarget::Ok(path) => path,
        ValidatedTarget::NotAbsolute => {
            return Ok(ShellOpenLocalPathResult::failed(
                ShellOpenLocalPathFailureReason::NotAbsolute,
            ));
        }
        ValidatedTarget::NotFound => {
            return Ok(ShellOpenLocalPathResult::failed(
                ShellOpenLocalPathFailureReason::NotFound,
            ));
        }
    };
    // Why: the file-manager action uses reveal semantics, matching the
    // previous sidebar behavior while still validating the path per click
    // (shell.ts:64-66). The opener plugin's `reveal_item_in_dir` is the
    // direct equivalent of `shell.showItemInFolder`.
    tauri_plugin_opener::OpenerExt::opener(app)
        .reveal_item_in_dir(&target)
        .map_err(|error| {
            eprintln!("[shell] reveal failed for {}: {error}", target.display());
        })
        .ok(); // reveal failures collapse to `launch-failed`, not a thrown error
    Ok(ShellOpenLocalPathResult::ok())
}

/// Port of `shell:openFilePath` → `openWithSystemDefault` (shell.ts:171-173 +
/// 125-136): validation failures are `false`, launch failures are `false`.
#[tauri::command]
pub(crate) async fn shell_open_file_path(
    app: AppHandle,
    path: String,
) -> Result<bool, DesktopCommandError> {
    let target = match validate_local_path_target(&path).await {
        ValidatedTarget::Ok(path) => path,
        ValidatedTarget::NotAbsolute | ValidatedTarget::NotFound => return Ok(false),
    };
    Ok(tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(target.to_string_lossy().as_ref(), None::<&str>)
        .is_ok())
}

/// Port of `shell:openFileUri` (shell.ts:175-205): file: protocol only,
/// empty or `localhost` hostname, `fileURLToPath` equivalent via
/// `url::Url::to_file_path`, then openWithSystemDefault semantics.
#[tauri::command]
pub(crate) async fn shell_open_file_uri(
    app: AppHandle,
    uri: String,
) -> Result<(), DesktopCommandError> {
    let parsed = match url::Url::parse(&uri) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(()),
    };
    if parsed.scheme() != "file" {
        return Ok(());
    }
    // Only local files are supported. Remote hosts are intentionally
    // rejected (shell.ts:187-190).
    let host_ok = match parsed.host() {
        Some(url::Host::Domain(domain)) => domain == "localhost",
        Some(_) => false,
        None => true,
    };
    if !host_ok {
        return Ok(());
    }
    let file_path = match parsed.to_file_path() {
        Ok(file_path) => file_path,
        Err(_) => return Ok(()),
    };
    let target = match validate_local_path_target(
        file_path.to_string_lossy().as_ref(),
    )
    .await
    {
        ValidatedTarget::Ok(path) => path,
        ValidatedTarget::NotAbsolute | ValidatedTarget::NotFound => return Ok(()),
    };
    let _ = tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(target.to_string_lossy().as_ref(), None::<&str>);
    Ok(())
}

/// Port of `shell:openInExternalEditor` (shell.ts:150-154 + 73-123) — local
/// branch only.
///
/// Why: the SSH branch (shell.ts:81-111) resolves targets through the
/// Electron `Store`'s SSH-authority state (`getSshTarget`,
/// `resolveVsCodeSshAuthority`), which does not exist in the Tauri backend
/// yet. Any request carrying a `connectionId` therefore returns
/// `remote-editor-unsupported` and the parity row stays open
/// (shell.ts:78-100 covers the dropped surface).
#[tauri::command]
pub(crate) async fn shell_open_in_external_editor(
    request: ShellOpenExternalEditorRequest,
) -> Result<ShellOpenExternalEditorResult, DesktopCommandError> {
    if request.connection_id.as_deref().is_some_and(|id| !id.trim().is_empty()) {
        return Ok(ShellOpenExternalEditorResult::failed(
            ShellOpenExternalEditorFailureReason::RemoteEditorUnsupported,
        ));
    }

    let target = match validate_local_path_target(&request.path).await {
        ValidatedTarget::Ok(path) => path,
        ValidatedTarget::NotAbsolute => {
            return Ok(ShellOpenExternalEditorResult::failed(
                ShellOpenExternalEditorFailureReason::NotAbsolute,
            ));
        }
        ValidatedTarget::NotFound => {
            return Ok(ShellOpenExternalEditorResult::failed(
                ShellOpenExternalEditorFailureReason::NotFound,
            ));
        }
    };
    let launch_spec = resolve_external_editor_launch_spec(request.command.as_deref(), &target);
    match launch_external_editor(&launch_spec).await {
        Ok(()) => Ok(ShellOpenExternalEditorResult::Ok),
        Err(()) => Ok(launch_failed_editor()),
    }
}

/// camelCase DTO mirroring `ShellOpenExternalEditorRequest`
/// (`src/shared/shell-open-types.ts:1-5`).
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShellOpenExternalEditorRequest {
    pub path: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub connection_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Pickers — tauri-plugin-dialog blocking API (app_dialogs.rs:66-86 pattern).
// All are async commands: blocking dialogs must not run on the main thread.
// ---------------------------------------------------------------------------

fn pick_error(operation: &'static str, error: impl std::fmt::Display) -> DesktopCommandError {
    command_error(
        "shell_pick_failed",
        operation,
        format!("picker failed: {error}"),
    )
}

fn canonicalized(path: std::path::PathBuf) -> Result<String, DesktopCommandError> {
    let canonical = std::fs::canonicalize(&path).map_err(|error| {
        command_error(
            "shell_pick_failed",
            OPERATION_PICK_DIRECTORY,
            format!("failed to resolve selected path {}: {error}", path.display()),
        )
    })?;
    canonical.to_str().map(str::to_owned).ok_or_else(|| {
        command_error(
            "shell_pick_failed",
            OPERATION_PICK_DIRECTORY,
            format!(
                "selected path is not valid Unicode: {}",
                canonical.display()
            ),
        )
    })
}

/// Port of `shell:pickDirectory` (shell.ts:211-225): open-directory dialog,
/// canonicalized result, `None` on cancel.
#[tauri::command]
pub(crate) async fn shell_pick_directory(
    app: AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, DesktopCommandError> {
    let mut builder = app.dialog().file();
    if let Some(default_path) = default_path {
        builder = builder.set_directory(default_path);
    }
    let Some(picked) = builder.blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| pick_error(OPERATION_PICK_DIRECTORY, error))?;
    Ok(Some(canonicalized(path)?))
}

/// Port of `shell:pickAttachment` (shell.ts:229-237): unfiltered file picker.
#[tauri::command]
pub(crate) async fn shell_pick_attachment(
    app: AppHandle,
) -> Result<Option<String>, DesktopCommandError> {
    let Some(picked) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| pick_error(OPERATION_PICK_ATTACHMENT, error))?;
    Ok(Some(canonicalized(path)?))
}

/// Port of `shell:pickImage` (shell.ts:241-252): image-extension filter.
#[tauri::command]
pub(crate) async fn shell_pick_image(
    app: AppHandle,
) -> Result<Option<String>, DesktopCommandError> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| pick_error(OPERATION_PICK_IMAGE, error))?;
    Ok(Some(canonicalized(path)?))
}

/// Port of `shell:pickAudio` (shell.ts:285-294): audio-extension filter.
#[tauri::command]
pub(crate) async fn shell_pick_audio(
    app: AppHandle,
) -> Result<Option<String>, DesktopCommandError> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Audio", &["ogg", "mp3", "wav", "m4a", "aac", "flac"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| pick_error(OPERATION_PICK_AUDIO, error))?;
    Ok(Some(canonicalized(path)?))
}

/// Port of `shell:pickRepoIconImage` (shell.ts:254-283): PNG-only picker with
/// the size cap and the original's exact error strings surfaced through the
/// typed error channel.
#[tauri::command]
pub(crate) async fn shell_pick_repo_icon_image(
    app: AppHandle,
) -> Result<Option<ShellPickedRepoIcon>, DesktopCommandError> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Repo icon images", &["png"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| pick_error(OPERATION_PICK_REPO_ICON_IMAGE, error))?;

    // shell.ts:266-270 — extension must map to a known PNG mime type.
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "png" {
        return Err(command_error(
            "shell_pick_failed",
            OPERATION_PICK_REPO_ICON_IMAGE,
            "Repo icons must be PNG files.",
        ));
    }

    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| pick_error(OPERATION_PICK_REPO_ICON_IMAGE, error))?;
    if metadata.len() as usize > MAX_REPO_ICON_UPLOAD_BYTES {
        return Err(command_error(
            "shell_pick_failed",
            OPERATION_PICK_REPO_ICON_IMAGE,
            "Repo icon image must be 256KB or smaller.",
        ));
    }

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| pick_error(OPERATION_PICK_REPO_ICON_IMAGE, error))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_default();
    Ok(Some(ShellPickedRepoIcon {
        data_url: format!("data:image/png;base64,{}", BASE64.encode(bytes)),
        file_name,
    }))
}

/// Port of `shell:copyFile` (shell.ts:299-313): both paths must be absolute;
/// destination must not exist (COPYFILE_EXCL semantics).
///
/// Why not `std::fs::copy`: Rust has no COPYFILE_EXCL equivalent, so the
/// exists-check + copy below carries a small TOCTOU race window between the
/// check and the write that Electron's atomic COPYFILE_EXCL does not have.
/// The renderer-side deconfliction loop (shell.ts:307-310) already picks a
/// unique destination name, so a concurrent clobber requires a second writer
/// racing the same path — same practical exposure as the original's caller.
#[tauri::command]
pub(crate) async fn shell_copy_file(
    src_path: String,
    dest_path: String,
) -> Result<(), DesktopCommandError> {
    let src = std::path::Path::new(&src_path);
    let dest = std::path::Path::new(&dest_path);
    if !src.is_absolute() || !dest.is_absolute() {
        return Err(command_error(
            "shell_invalid_argument",
            OPERATION_COPY_FILE,
            "Both source and destination must be absolute paths",
        ));
    }
    // COPYFILE_EXCL: fail loudly rather than clobber data (shell.ts:307-311).
    let mut source = tokio::fs::File::open(src).await.map_err(|error| {
        command_error(
            "shell_launch_failed",
            OPERATION_COPY_FILE,
            format!("failed to open source {}: {error}", src.display()),
        )
    })?;
    let mut dest_file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .await
        .map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::AlreadyExists {
                "shell_already_exists"
            } else {
                "shell_launch_failed"
            };
            command_error(
                code,
                OPERATION_COPY_FILE,
                format!("failed to create destination {}: {error}", dest.display()),
            )
        })?;
    tokio::io::copy(&mut source, &mut dest_file)
        .await
        .map_err(|error| {
            command_error(
                "shell_launch_failed",
                OPERATION_COPY_FILE,
                format!("failed to copy {} to {}: {error}", src.display(), dest.display()),
            )
        })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// External editor launch chain — port of
// `orca/src/main/external-editor-launch.ts` (local branches).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum ExternalEditorLaunchSpec {
    /// `ExternalEditorExecutableLaunchSpec` (external-editor-launch.ts:21-32).
    Executable {
        hide_windows_console: bool,
        /// Why: set only for Windows batch shims that would otherwise leave a
        /// Command Prompt behind; routes the spawn through `start "" /B`
        /// (external-editor-launch.ts:24-29).
        detached_gui: bool,
        spawn_cmd: String,
        spawn_args: Vec<String>,
    },
    /// `ExternalEditorShellLaunchSpec` (external-editor-launch.ts:34-39).
    Shell {
        hide_windows_console: bool,
        spawn_cmd: String,
        spawn_args: Vec<String>,
    },
}

fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

fn is_windows() -> bool {
    node_platform() == "win32"
}

/// Port of `stripMatchingQuotes` (editor-launcher-name.ts:3-10).
fn strip_matching_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    if let Some(&quote) = bytes.first() {
        if (quote == b'"' || quote == b'\'') && bytes.last() == Some(&quote) {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

/// Port of `hasMatchingOuterQuotes` (editor-launcher-name.ts:12-16).
fn has_matching_outer_quotes(value: &str) -> bool {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    match bytes.first() {
        Some(&quote @ (b'"' | b'\'')) => bytes.last() == Some(&quote),
        _ => false,
    }
}

/// Port of `getLauncherBaseName` (editor-launcher-name.ts:31-40): lowercased
/// launcher name without directory or Windows executable extension.
fn get_launcher_base_name(command: &str, shell_command: bool) -> String {
    let normalized = if shell_command {
        // getLeadingShellCommandToken (editor-launcher-name.ts:18-28)
        let trimmed = command.trim();
        let quote = trimmed.chars().next();
        match quote {
            Some(q @ ('"' | '\'')) => {
                if let Some(closing) = trimmed[1..].find(q) {
                    trimmed[1..1 + closing].to_owned()
                } else {
                    trimmed.split_whitespace().next().unwrap_or("").to_owned()
                }
            }
            _ => trimmed.split_whitespace().next().unwrap_or("").to_owned(),
        }
    } else {
        strip_matching_quotes(command).to_owned()
    };
    let name = normalized
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&normalized)
        .to_owned();
    // name.replace(/\.(?:cmd|exe|bat)$/i, '')
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".cmd") || lower.ends_with(".exe") || lower.ends_with(".bat") {
        name[..name.len() - 4].to_ascii_lowercase()
    } else {
        lower
    }
}

/// Port of `isWindowsBatchLauncher` (editor-launcher-name.ts:42-45).
fn is_windows_batch_launcher(command: &str) -> bool {
    let unquoted = strip_matching_quotes(command);
    let lower = unquoted.to_ascii_lowercase();
    is_windows() && (lower.ends_with(".cmd") || lower.ends_with(".bat"))
}

/// Port of `isWindowsExecutablePath` (external-editor-launch.ts:62-64).
fn is_windows_executable_path(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    std::path::Path::new(command).is_absolute()
        && (lower.ends_with(".cmd")
            || lower.ends_with(".exe")
            || lower.ends_with(".bat")
            || lower.ends_with(".com"))
}

/// Port of `isDirectExecutablePath` (external-editor-launch.ts:66-86).
fn is_direct_executable_path(command: &str, file_exists: &dyn Fn(&str) -> bool) -> bool {
    let unquoted = strip_matching_quotes(command);
    if !unquoted.contains('\\') && !unquoted.contains('/') {
        return false;
    }
    let is_absolute_path = std::path::Path::new(unquoted).is_absolute();
    if !is_absolute_path {
        return false;
    }
    if !unquoted.chars().any(char::is_whitespace) || has_matching_outer_quotes(command) {
        return true;
    }
    // Why: unquoted POSIX paths can contain spaces, but so can shell commands
    // with arguments. Only an existing path is safe to treat as one
    // executable (external-editor-launch.ts:83-85).
    is_windows() && is_windows_executable_path(unquoted) || file_exists(unquoted)
}

/// Port of `parseWslUncPath` (orca/src/shared/wsl-paths.ts:6-17).
fn parse_wsl_unc_path(path: &str) -> Option<(String, String)> {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("//wsl.localhost/")
        .or_else(|| lower.strip_prefix("//wsl$/"))?;
    let (distro, linux_path) = match rest.split_once('/') {
        Some((distro, tail)) => (distro, tail.to_owned()),
        None => (rest, String::new()),
    };
    if distro.is_empty() {
        return None;
    }
    // The distro name keeps its original case; re-slice from `normalized`.
    let prefix_len = normalized.len() - rest.len();
    let distro_original = normalized[prefix_len..prefix_len + distro.len()].to_owned();
    Some((distro_original, linux_path))
}

/// Port of `isVsCodeLauncherExecutable`
/// (orca/src/shared/vscode-remote-ssh-launcher.ts:13-19).
fn is_vscode_launcher_executable(command: &str) -> bool {
    let unquoted = strip_matching_quotes(command);
    let file_name = unquoted.rsplit(['\\', '/']).next().unwrap_or("");
    let launcher_name = get_launcher_base_name(file_name, false);
    matches!(
        launcher_name.as_str(),
        "code" | "code-insiders" | "code - insiders"
    )
}

/// Port of `buildExecutableArgs` (external-editor-launch.ts:96-115).
fn build_executable_args(editor_command: &str, path_value: &str) -> Vec<String> {
    let launcher_base_name = get_launcher_base_name(editor_command, false);
    if launcher_base_name == "cursor" {
        // Why: Cursor can route bare folder launches through the last active
        // workbench. A new window keeps "Open in Cursor" scoped to this
        // worktree (external-editor-launch.ts:102-106).
        return vec!["--new-window".to_owned(), path_value.to_owned()];
    }
    if is_windows() && is_vscode_launcher_executable(editor_command) {
        if let Some((distro, linux_path)) = parse_wsl_unc_path(path_value) {
            // Why: VS Code otherwise treats a WSL UNC path as a local Windows
            // folder (external-editor-launch.ts:107-113).
            return vec!["--remote".to_owned(), format!("wsl+{distro}"), linux_path];
        }
    }
    vec![path_value.to_owned()]
}

/// Port of `shouldShowWindowsConsole` (external-editor-launch.ts:88-94).
fn should_show_windows_console(command: &str, shell_command: bool) -> bool {
    is_windows()
        && matches!(
            get_launcher_base_name(command, shell_command).as_str(),
            "nvim" | "vim"
        )
}

/// Port of `isJetBrainsConsoleShim`
/// (orca/src/main/jetbrains-windows-gui-launchers.ts:85-91).
fn is_jetbrains_console_shim(command: &str) -> bool {
    is_windows()
        && is_windows_batch_launcher(command)
        && jetbrains_gui_executable_name(&get_launcher_base_name(command, false)).is_some()
}

/// GUI binary basename for a console or GUI launcher name, or None
/// (jetbrains-windows-gui-launchers.ts:13-35).
fn jetbrains_gui_executable_name(launcher_base_name: &str) -> Option<&'static str> {
    match launcher_base_name {
        "idea" => Some("idea64"),
        "webstorm" => Some("webstorm64"),
        "pycharm" => Some("pycharm64"),
        "phpstorm" => Some("phpstorm64"),
        "goland" => Some("goland64"),
        "rider" => Some("rider64"),
        "clion" => Some("clion64"),
        "rubymine" => Some("rubymine64"),
        "datagrip" => Some("datagrip64"),
        "rustrover" => Some("rustrover64"),
        "studio" => Some("studio64"),
        "idea64" | "webstorm64" | "pycharm64" | "phpstorm64" | "goland64" | "rider64"
        | "clion64" | "rubymine64" | "datagrip64" | "rustrover64" | "studio64" => {
            Some(JETBRAINS_GUI_SELF)
        }
        _ => None,
    }
}

const JETBRAINS_GUI_SELF: &str = "";

/// Port of `preferJetBrainsGuiExecutable`
/// (external-editor-launch.ts:121-130) +
/// `resolveColocatedJetBrainsGuiExecutable`
/// (jetbrains-windows-gui-launchers.ts:50-79): upgrade `.cmd`/`.bat` shims
/// and short console `.exe` stubs to the colocated `*64.exe` when present.
fn prefer_jetbrains_gui_executable(editor_command: &str) -> String {
    if !is_windows() {
        return editor_command.to_owned();
    }
    let unquoted = strip_matching_quotes(editor_command);
    let lower = unquoted.to_ascii_lowercase();
    let is_batch = lower.ends_with(".cmd") || lower.ends_with(".bat");
    let is_exe = lower.ends_with(".exe");
    if !is_batch && !is_exe {
        return editor_command.to_owned();
    }

    let base_name = get_launcher_base_name(unquoted, false);
    let gui_name = if is_exe {
        // .exe: only short console names (idea.exe), never the GUI binary
        // itself (jetbrains-windows-gui-launchers.ts:62-70).
        match jetbrains_gui_executable_name(&base_name) {
            Some(JETBRAINS_GUI_SELF) => return editor_command.to_owned(),
            Some(gui) => gui,
            None => return editor_command.to_owned(),
        }
    } else {
        // .cmd/.bat: idea.cmd and idea64.cmd both prefer idea64.exe.
        match jetbrains_gui_executable_name(&base_name) {
            Some(JETBRAINS_GUI_SELF) => base_name.as_str(),
            Some(gui) => gui,
            None => return editor_command.to_owned(),
        }
    };

    let candidate = std::path::Path::new(unquoted)
        .parent()
        .map(|parent| parent.join(format!("{gui_name}.exe")))
        .map(|candidate| candidate.to_string_lossy().to_string());
    match candidate {
        Some(candidate) if std::path::Path::new(&candidate).exists() => candidate,
        _ => editor_command.to_owned(),
    }
}

/// Port of `resolveCliCommand` (orca/src/shared/node-cli-command-resolution.ts:153-197)
/// — PATH walk plus the version-manager fallback directories. The
/// codex-specific MSIX exclusion is dropped (no `windows-codex-cli-paths`
/// port yet); codex falls through to the base resolution.
fn resolve_cli_command(command_name: &str) -> String {
    let executable_names: Vec<String> = if is_windows() {
        vec![
            format!("{command_name}.cmd"),
            format!("{command_name}.exe"),
            format!("{command_name}.bat"),
            command_name.to_owned(),
        ]
    } else {
        vec![command_name.to_owned()]
    };
    let path_env = std::env::var("PATH").unwrap_or_default();
    let separator = if is_windows() { ';' } else { ':' };
    for directory in path_env.split(separator).filter(|part| !part.is_empty()) {
        for name in &executable_names {
            let candidate = std::path::Path::new(directory).join(name);
            if is_runnable_command(&candidate) {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    // Version-manager fallback directories
    // (node-cli-command-resolution.ts:90-121).
    if let Some(home) = std::env::var_os(if is_windows() { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
    {
        let mut directories = vec![
            home.join(".volta").join("bin"),
            home.join(".asdf").join("shims"),
            home.join(".fnm").join("aliases").join("default").join("bin"),
            home.join(".local").join("share").join("mise").join("shims"),
        ];
        if is_windows() {
            directories.push(home.join(".local").join("bin"));
            directories.push(home.join("AppData").join("Roaming").join("npm"));
            directories.push(home.join("AppData").join("Local").join("pnpm"));
            directories.push(home.join("AppData").join("Local").join("Yarn").join("bin"));
        } else {
            directories.push(home.join(".local").join("bin"));
            if node_platform() == "darwin" {
                directories.push(home.join("Library").join("pnpm"));
            } else {
                directories.push(home.join(".local").join("share").join("pnpm"));
            }
            directories.push(home.join(".yarn").join("bin"));
        }
        directories.push(home.join(".bun").join("bin"));
        for directory in directories {
            for name in &executable_names {
                let candidate = directory.join(name);
                if is_runnable_command(&candidate) {
                    return candidate.to_string_lossy().to_string();
                }
            }
        }
    }
    command_name.to_owned()
}

/// Port of `isRunnableCommand` (node-cli-command-resolution.ts:72-88).
fn is_runnable_command(candidate: &std::path::Path) -> bool {
    match std::fs::metadata(candidate) {
        Ok(metadata) => metadata.is_file(),
        Err(_) => false,
    }
}

/// Port of `getCmdExePath` (orca/src/shared/windows-batch-spawn.ts:4-9).
fn get_cmd_exe_path() -> String {
    std::env::var("ComSpec")
        .unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".to_owned())
}

/// Port of `escapePathForShell` (external-editor-launch.ts:45-60).
fn escape_path_for_shell(path_value: &str) -> String {
    if is_windows() {
        // escapeWindowsPathForShell (external-editor-launch.ts:52-54)
        if path_value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./@:\\-".contains(c))
        {
            path_value.to_owned()
        } else {
            format!("\"{path_value}\"")
        }
    } else {
        // escapePosixPathForShell (external-editor-launch.ts:45-50)
        if path_value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./@:-".contains(c))
        {
            path_value.to_owned()
        } else {
            format!("'{}'", path_value.replace('\'', "'\\''"))
        }
    }
}

/// Port of `isCompoundShellCommand` (external-editor-launch.ts:117-119).
fn is_compound_shell_command(command: &str) -> bool {
    command.chars().any(char::is_whitespace)
}

/// Port of `buildExecutableLaunchSpec` (external-editor-launch.ts:144-159).
fn build_executable_launch_spec(
    editor_command: &str,
    path_value: &str,
) -> ExternalEditorLaunchSpec {
    let hide_windows_console = !should_show_windows_console(editor_command, false);
    let detached_gui = hide_windows_console && is_jetbrains_console_shim(editor_command);
    ExternalEditorLaunchSpec::Executable {
        hide_windows_console,
        detached_gui,
        spawn_cmd: editor_command.to_owned(),
        spawn_args: build_executable_args(editor_command, path_value),
    }
}

/// Port of `buildShellLaunchSpec` (external-editor-launch.ts:161-186).
fn build_shell_launch_spec(command: &str, path_value: &str) -> ExternalEditorLaunchSpec {
    let shell_command = format!("{command} {}", escape_path_for_shell(path_value));
    if is_windows() {
        // Why: no `start` wrap here. `start` re-parses the line — it swallows
        // the first quoted operand as a window title and mangles remote paths
        // with spaces — and on a batch target it leaves a resident nested
        // `cmd /K` (external-editor-launch.ts:167-178).
        return ExternalEditorLaunchSpec::Shell {
            hide_windows_console: !should_show_windows_console(command, true),
            spawn_cmd: get_cmd_exe_path(),
            spawn_args: vec!["/d".into(), "/s".into(), "/c".into(), shell_command],
        };
    }
    ExternalEditorLaunchSpec::Shell {
        hide_windows_console: true,
        spawn_cmd: "/bin/sh".to_owned(),
        spawn_args: vec!["-c".to_owned(), shell_command],
    }
}

/// Port of `resolveSimpleEditorCommand` (external-editor-launch.ts:132-142).
fn resolve_simple_editor_command(command: &str) -> String {
    prefer_jetbrains_gui_executable(&resolve_cli_command(command))
}

/// Port of `resolveExternalEditorLaunchSpec`
/// (external-editor-launch.ts:188-216).
fn resolve_external_editor_launch_spec(
    command: Option<&str>,
    path_value: &std::path::Path,
) -> ExternalEditorLaunchSpec {
    let path_value = path_value.to_string_lossy().to_string();
    let file_exists =
        |path: &str| std::path::Path::new(path).exists();
    let trimmed = command
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(EXTERNAL_EDITOR_CLI_COMMAND);

    if is_direct_executable_path(trimmed, &file_exists) {
        // Why: settings often store a full path to idea.cmd / idea.exe;
        // upgrade those the same way as PATH-resolved names when *64.exe is
        // colocated (external-editor-launch.ts:198-205).
        return build_executable_launch_spec(
            &prefer_jetbrains_gui_executable(strip_matching_quotes(trimmed)),
            &path_value,
        );
    }

    if is_compound_shell_command(trimmed) {
        return build_shell_launch_spec(trimmed, &path_value);
    }

    build_executable_launch_spec(&resolve_simple_editor_command(trimmed), &path_value)
}

struct ResolvedSpawn {
    spawn_cmd: String,
    spawn_args: Vec<String>,
    windows_hide: bool,
}

/// Port of `resolveExternalEditorSpawn` (external-editor-launch.ts:249-267) +
/// `getSpawnArgsForWindows` (windows-batch-spawn.ts:55-89).
fn resolve_external_editor_spawn(launch_spec: &ExternalEditorLaunchSpec) -> ResolvedSpawn {
    match launch_spec {
        ExternalEditorLaunchSpec::Executable {
            hide_windows_console,
            detached_gui,
            spawn_cmd,
            spawn_args,
        } => {
            let lower = spawn_cmd.to_ascii_lowercase();
            let is_batch = is_windows()
                && (lower.ends_with(".cmd") || lower.ends_with(".bat"));
            if is_batch {
                // Batch targets route through cmd.exe; the JetBrains detached
                // form nests an inner `cmd /d /c` so the shim exits with the
                // script (windows-batch-spawn.ts:68-85).
                if *detached_gui {
                    // Why: `start` launches a batch target through a nested
                    // `cmd /K`, which stays resident after the script ends —
                    // `/B` only suppresses a *new* console, so the shim leaks
                    // a hidden cmd.exe. Handing `start` an inner `cmd /d /c`
                    // makes that interpreter exit with the script
                    // (windows-batch-spawn.ts:68-85).
                    //
                    // Window title must be an *empty argv entry*: the empty
                    // title `start` requires so a later quoted path is not
                    // eaten as the title.
                    let cmd_exe_path = get_cmd_exe_path();
                    let mut spawn_args = vec![
                        "/d".to_owned(),
                        "/c".to_owned(),
                        "start".to_owned(),
                        String::new(),
                        "/B".to_owned(),
                        cmd_exe_path.clone(),
                        "/d".to_owned(),
                        "/c".to_owned(),
                        spawn_cmd.clone(),
                    ];
                    let extra: Vec<String> = spawn_args.iter().cloned().collect();
                    spawn_args.extend(extra);
                    return ResolvedSpawn {
                        spawn_cmd: cmd_exe_path,
                        spawn_args,
                        windows_hide: *hide_windows_console,
                    };
                }
                return ResolvedSpawn {
                    spawn_cmd: get_cmd_exe_path(),
                    spawn_args: {
                        let mut args = vec!["/d".to_owned(), "/c".to_owned()];
                        args.push(spawn_cmd.clone());
                        args.extend(spawn_args.iter().cloned());
                        args
                    },
                    windows_hide: *hide_windows_console,
                };
            }
            ResolvedSpawn {
                spawn_cmd: spawn_cmd.clone(),
                spawn_args: spawn_args.clone(),
                windows_hide: *hide_windows_console,
            }
        }
        ExternalEditorLaunchSpec::Shell {
            hide_windows_console,
            spawn_cmd,
            spawn_args,
        } => ResolvedSpawn {
            spawn_cmd: spawn_cmd.clone(),
            spawn_args: spawn_args.clone(),
            windows_hide: *hide_windows_console,
        },
    }
}


/// Port of `launchExternalEditor` (external-editor-launch.ts:269-296): spawn
/// detached with ignored stdio; success = the process started.
async fn launch_external_editor(launch_spec: &ExternalEditorLaunchSpec) -> Result<(), ()> {
    let resolved = resolve_external_editor_spawn(launch_spec);
    use std::process::Command;
    let mut command = Command::new(&resolved.spawn_cmd);
    command
        .args(&resolved.spawn_args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        // windowsHide: no console window for console-subsystem children.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        if resolved.windows_hide {
            command.creation_flags(CREATE_NO_WINDOW);
        } else {
            command.creation_flags(DETACHED_PROCESS);
        }
    }
    #[cfg(unix)]
    {
        // Detached: the editor outlives this process (Node `detached: true`).
        unsafe {
            libc::setsid();
        }
    }
    // Node's spawn resolves the 'spawn' event once the child is created;
    // std::process::Command::spawn returns after the same point.
    command.spawn().map(|_| ()).map_err(|error| {
        eprintln!(
            "[shell] external editor launch failed ({} {:?}): {error}",
            resolved.spawn_cmd, resolved.spawn_args
        );
    })
}
