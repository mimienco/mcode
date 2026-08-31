//! Menu/tray → renderer intent events, ported from the Electron app-menu
//! and tray flows (`openSettingsFromSystemMenu`, feature-tour/setup-guide/
//! crash-report senders). Each command forwards the intent to the main
//! window's renderer as a typed Tauri event; settings additionally keeps an
//! untimed one-shot slot so a cold renderer can pull it on mount
//! (`ui:consumePendingOpenSettings` semantics).

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime};

const MAIN_WINDOW_LABEL: &str = "main";

pub const OPEN_SETTINGS_EVENT: &str = "ui:openSettings";
pub const OPEN_SETUP_GUIDE_EVENT: &str = "ui:openSetupGuide";
pub const OPEN_FEATURE_TOUR_EVENT: &str = "ui:openFeatureTour";
pub const OPEN_CRASH_REPORT_EVENT: &str = "ui:openCrashReport";

#[derive(Default)]
struct PendingOpenSettings(Mutex<bool>);

fn emit_to_main_window<R: Runtime>(app: &AppHandle<R>, event: &str) -> bool {
    match app.get_webview_window(MAIN_WINDOW_LABEL) {
        Some(window) => window.emit(event, ()).is_ok(),
        None => false,
    }
}

/// Tray/menu "Settings…" → show the window, push the event, and keep an
/// untimed one-shot so a not-yet-mounted renderer still opens Settings
/// (mirrors `openSettingsFromSystemMenu` + `pendingOpenSettings.mark`).
pub fn open_settings<R: Runtime>(app: &AppHandle<R>) -> bool {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(state) = app.try_state::<PendingOpenSettings>() {
        *state.0.lock().expect("pending open-settings poisoned") = true;
    }
    emit_to_main_window(app, OPEN_SETTINGS_EVENT)
}

/// One-shot consume mirroring `ui:consumePendingOpenSettings`.
pub fn consume_pending_open_settings<R: Runtime>(app: &AppHandle<R>) -> bool {
    match app.try_state::<PendingOpenSettings>() {
        Some(state) => {
            let mut pending = state.0.lock().expect("pending open-settings poisoned");
            std::mem::take(&mut *pending)
        }
        None => false,
    }
}

pub fn open_setup_guide<R: Runtime>(app: &AppHandle<R>) -> bool {
    emit_to_main_window(app, OPEN_SETUP_GUIDE_EVENT)
}

pub fn open_feature_tour<R: Runtime>(app: &AppHandle<R>) -> bool {
    emit_to_main_window(app, OPEN_FEATURE_TOUR_EVENT)
}

pub fn open_crash_report<R: Runtime>(app: &AppHandle<R>) -> bool {
    emit_to_main_window(app, OPEN_CRASH_REPORT_EVENT)
}

/// Manages the pending-settings slot once per app run.
pub fn register<R: Runtime>(app: &AppHandle<R>) {
    app.manage(PendingOpenSettings::default());
}

#[cfg(test)]
mod tests {
    // The event names and payload shapes are asserted by the renderer
    // useIpcEvents suite; the slot logic is exercised via the Tauri command
    // integration. Pure parsing lives in skill_share_deep_link tests.
    #[test]
    fn event_names_match_electron_channels() {
        assert_eq!(super::OPEN_SETTINGS_EVENT, "ui:openSettings");
        assert_eq!(super::OPEN_SETUP_GUIDE_EVENT, "ui:openSetupGuide");
        assert_eq!(super::OPEN_FEATURE_TOUR_EVENT, "ui:openFeatureTour");
        assert_eq!(super::OPEN_CRASH_REPORT_EVENT, "ui:openCrashReport");
    }
}
