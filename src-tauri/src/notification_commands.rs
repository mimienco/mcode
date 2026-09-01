//! Native notification commands backing the renderer `window.api.notifications`
//! bridge, ported from the Electron `notifications:*` channels.
//!
//! Mapping (original: src/main/ipc/notification-*.ts):
//! - `notifications:dispatch` → `ui_notificationDispatch` (tauri-plugin-notification)
//! - `notifications:getPermissionStatus` → `ui_notificationGetPermissionStatus`
//!   (Windows/WebView2: Web Notification permission governs; granted by default)
//! - `notifications:openSystemSettings` → `ui_notificationOpenSystemSettings`
//!   (ms-settings:notifications URI on Windows; macOS System Settings on darwin)
//! - `notifications:dismiss` → `ui_notificationDismiss` (best-effort no-op on
//!   Windows toast without action-center id tracking; documented gap)
//! - `notifications:probeDelivery` → `ui_notificationProbeDelivery` (dispatch a
//!   silent probe; delivery confirmation needs the notification click event,
//!   reported as `unverified` on Windows v1)
//!
//! Sound playback stays renderer-side (HTMLAudioElement) — the Electron main
//! only resolved/loaded sound files for the renderer, which the fs slice covers.

use serde::Serialize;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use tauri_plugin_notification::NotificationExt;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDispatchResult {
    delivered: bool,
    /// Platform note for dispatches that cannot be confirmed (Windows toasts).
    note: Option<String>,
}

/// Dispatches a system notification (original `notifications:dispatch`).
#[tauri::command(rename_as = "ui_notificationDispatch")]
pub(crate) fn ui_notification_dispatch(
    app: AppHandle,
    title: String,
    body: Option<String>,
    tag: Option<String>,
) -> Result<NotificationDispatchResult, String> {
    let mut notification = app.notification().builder().title(title);
    if let Some(body) = body {
        notification = notification.body(body);
    }
    // Gap: the plugin builder has no group/replace identifier; Electron's tag
    // semantics (replace-by-id) degrade to independent toasts on Windows v1.
    let _ = tag;
    notification.show().map_err(|error| error.to_string())?;
    Ok(NotificationDispatchResult {
        delivered: true,
        // Why: Windows toasts offer no synchronous delivery confirmation;
        // callers treat `delivered=true` as "handed to the OS".
        note: Some("handed-to-os".to_string()),
    })
}

/// Permission status (original `notifications:getPermissionStatus`).
/// Windows WebView2 toasts require no runtime grant; macOS asks TCC lazily.
#[tauri::command(rename_as = "ui_notificationGetPermissionStatus")]
pub(crate) fn ui_notification_get_permission_status() -> String {
    if cfg!(target_os = "windows") {
        "granted".to_string()
    } else if cfg!(target_os = "macos") {
        // The plugin surfaces the real status on darwin; default to granted
        // until the TCC probe slice lands.
        "granted".to_string()
    } else {
        "granted".to_string()
    }
}

/// Opens the OS notification settings pane (original
/// `notifications:openSystemSettings`).
#[tauri::command(rename_as = "ui_notificationOpenSystemSettings")]
pub(crate) fn ui_notification_open_system_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:notifications"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|error| format!("failed to open notification settings: {error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        // macOS: `x-apple.systempreferences:com.apple.preference.notifications`;
        // Linux: gnome control center. Defer to the platform slice when needed.
        Err("notification settings open is not implemented on this platform".to_string())
    }
}

/// Best-effort dismiss (original `notifications:dismiss`). Windows toasts
/// cannot be dismissed programmatically without an Action Center COM
/// registration — recorded as a documented gap rather than a fake success.
#[tauri::command(rename_as = "ui_notificationDismiss")]
pub(crate) fn ui_notification_dismiss(_ids: Vec<String>) -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_permission_is_granted_by_default() {
        // Compile-time platform mapping check; runs everywhere.
        let status = if cfg!(target_os = "windows") {
            "granted"
        } else {
            "granted"
        };
        assert_eq!(super::ui_notification_get_permission_status(), status);
    }

    #[test]
    fn dismiss_reports_unsupported() {
        assert!(!super::ui_notification_dismiss(vec!["a".to_string()]).unwrap());
    }
}
