//! System tray icon and context menu.
//!
//! Port of the original Electron `src/main/tray/system-tray.ts` tray surface:
//! a notification-area icon (menu bar item on macOS) with a context menu of
//! `Open MCode` / `Settings` / `Check for Updates...` / `Quit`, tooltip
//! `MCode`, and a left-click that restores the main window on Windows.
//!
//! Like the original, installation is idempotent: installing twice returns
//! [`TrayError::AlreadyInstalled`] instead of stacking a duplicate icon. The
//! original held the `Tray` in module scope so the garbage collector could not
//! drop it; tauri ties the tray to the app handle's lifetime, so the icon
//! survives as long as the app does.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::events::TrayEvent;

/// The original tooltip (`baseTooltip()`); dev instances append their own
/// marker at the app layer via the menu header, so the base stays plain.
pub const TRAY_TOOLTIP: &str = "MCode";

/// Bundled icon resource path, relative to the crate, included via
/// `tauri::include_image!`.
pub const TRAY_ICON_PATH: &str = "./assets/icon.png";

/// Errors from tray installation, each with a stable string code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayError {
    /// A tray is already installed for this process; repeated calls must not
    /// stack duplicate icons (same guarantee as the original module-scope
    /// `tray` guard).
    AlreadyInstalled,
    /// The bundled tray icon could not be loaded or decoded.
    IconUnavailable,
    /// A native menu item could not be created.
    MenuUnavailable,
    /// The platform refused to register the tray icon.
    RegistrationFailed,
}

impl TrayError {
    pub fn code(self) -> &'static str {
        match self {
            Self::AlreadyInstalled => "tray_already_installed",
            Self::IconUnavailable => "tray_icon_unavailable",
            Self::MenuUnavailable => "tray_menu_unavailable",
            Self::RegistrationFailed => "tray_registration_failed",
        }
    }
}

impl fmt::Display for TrayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyInstalled => "tray icon is already installed",
            Self::IconUnavailable => "tray icon resource is unavailable",
            Self::MenuUnavailable => "tray context menu could not be created",
            Self::RegistrationFailed => "platform rejected tray icon registration",
        })
    }
}

impl std::error::Error for TrayError {}

/// Whether this process has already installed the tray. The original used a
/// module-scope `tray` variable for the same purpose.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Installs the system tray icon.
///
/// `on_event` is invoked with a [`TrayEvent`] whenever the user activates the
/// tray icon or picks a menu item. It runs on the main thread; it must not
/// block. The crate never acts on events itself — the app decides, which
/// keeps the shutdown protocol (quit latch, save/discard veto) in `mcode-app`.
///
/// Windows: left-click emits [`TrayEvent::ShowMainWindow`] (the original
/// `tray.on('click', opts.onOpen)` gesture for restoring a minimized-to-tray
/// app). macOS: left-click opens the attached menu, exactly like the original
/// status item, so no click event is emitted there.
pub fn install_tray(
    app: &AppHandle,
    on_event: impl Fn(TrayEvent) + Send + Sync + 'static,
) -> Result<(), TrayError> {
    // Idempotency: check-and-set as one step so concurrent installs cannot
    // both pass the guard.
    if INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(TrayError::AlreadyInstalled);
    }

    let result = install_tray_inner(app, on_event);
    if result.is_err() {
        // Release the guard so a transient failure can be retried.
        INSTALLED.store(false, Ordering::Release);
    }
    result
}

fn install_tray_inner(
    app: &AppHandle,
    on_event: impl Fn(TrayEvent) + Send + Sync + 'static,
) -> Result<(), TrayError> {
    // The bundled app icon, decoded at compile time from the crate's assets.
    // A load failure here is a build/packaging defect, not a runtime state the
    // app can recover from, so it maps to a typed error instead of a panic.
    let icon = tauri::include_image!("./assets/icon.png");
    if icon.rgba().is_empty() {
        return Err(TrayError::IconUnavailable);
    }

    let menu = build_menu(app)?;

    let handler = Arc::new(on_event);
    let click_handler = Arc::clone(&handler);

    let mut builder = TrayIconBuilder::with_id("mcode-tray")
        .icon(icon)
        .tooltip(TRAY_TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            // Map the native menu id back to our event vocabulary; unknown
            // ids are ignored so a future menu cannot inject events the
            // app has not opted into.
            if let Some(tray_event) = TrayEvent::from_id(event.id().as_ref()) {
                handler(tray_event);
            }
            let _ = app;
        });

    // Windows: left-click on the icon is the restore gesture. macOS handles
    // the click natively by opening the menu (menu_on_left_click already
    // covers it), and emitting a show event there would double-fire.
    if cfg!(windows) {
        builder = builder.on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                click_handler(TrayEvent::ShowMainWindow);
            }
        });
    }

    builder
        .build(app)
        .map(|_| ())
        .map_err(|_| TrayError::RegistrationFailed)
}

/// Builds the context menu with the original labels: `Open MCode`,
/// `Settings`, `Check for Updates...`, `Quit`. The original gated
/// Settings/Updates behind `process.platform === 'darwin'`; the event seam
/// lets the app apply any platform policy, so the menu exposes all items
/// uniformly.
fn build_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, TrayError> {
    let mut items: Vec<MenuItem<tauri::Wry>> = Vec::new();
    for event in [
        TrayEvent::ShowMainWindow,
        TrayEvent::OpenSettings,
        TrayEvent::CheckForUpdates,
        TrayEvent::QuitRequested,
    ] {
        let item = MenuItem::with_id(app, event.id(), event.default_label(), true, None::<&str>)
            .map_err(|_| TrayError::MenuUnavailable)?;
        items.push(item);
    }

    let item_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<_>)
        .collect();
    Menu::with_items(app, &item_refs).map_err(|_| TrayError::MenuUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable_strings() {
        assert_eq!(TrayError::AlreadyInstalled.code(), "tray_already_installed");
        assert_eq!(TrayError::IconUnavailable.code(), "tray_icon_unavailable");
        assert_eq!(TrayError::MenuUnavailable.code(), "tray_menu_unavailable");
        assert_eq!(
            TrayError::RegistrationFailed.code(),
            "tray_registration_failed"
        );
        // Display text exists and is non-empty for every variant.
        for error in [
            TrayError::AlreadyInstalled,
            TrayError::IconUnavailable,
            TrayError::MenuUnavailable,
            TrayError::RegistrationFailed,
        ] {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn already_installed_error_is_distinct_from_registration_failure() {
        // Double-install detection must surface as AlreadyInstalled, never as
        // a registration failure, so callers can distinguish the two states.
        assert_ne!(
            TrayError::AlreadyInstalled.code(),
            TrayError::RegistrationFailed.code()
        );
    }

    #[test]
    fn install_guard_flips_on_and_releases_on_error() {
        // Exercises the same AtomicBool the real install uses, without a live
        // app handle: the compare-exchange must admit exactly one installer
        // and a released guard must admit the next one.
        assert!(
            INSTALLED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        );
        assert!(
            INSTALLED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        );
        INSTALLED.store(false, Ordering::Release);
        assert!(
            INSTALLED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        );
        INSTALLED.store(false, Ordering::Release);
    }

    #[test]
    fn tooltip_matches_original() {
        assert_eq!(TRAY_TOOLTIP, "MCode");
    }
}
