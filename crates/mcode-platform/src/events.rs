//! Tray events emitted to the application.
//!
//! Mirrors the original Electron tray callbacks (`onOpen`, `onOpenSettings`,
//! `onCheckForUpdates`, `onQuit` in `src/main/tray/system-tray.ts`). The app
//! layer owns the reaction — including the quit latch and shutdown protocol —
//! this crate only reports what the user clicked.

/// User intent from the tray icon or its context menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayEvent {
    /// "Open MCode" menu item, or a Windows left-click on the tray icon.
    /// The app must restore/show/focus the main window.
    ShowMainWindow,
    /// "Settings" menu item. The app must restore the main window and open
    /// its Settings surface.
    OpenSettings,
    /// "Check for Updates..." menu item. The app must show the main window
    /// first so updater status renders visibly, then run the check.
    CheckForUpdates,
    /// "Quit" menu item. The app must run its shutdown protocol (quit latch,
    /// teardown) before exiting; the tray never quits anything itself.
    QuitRequested,
}

impl TrayEvent {
    /// Stable identifier, matching the original menu item identity. Used as
    /// the tauri menu item id and for event mapping.
    pub fn id(self) -> &'static str {
        match self {
            Self::ShowMainWindow => "show_main_window",
            Self::OpenSettings => "open_settings",
            Self::CheckForUpdates => "check_for_updates",
            Self::QuitRequested => "quit_requested",
        }
    }

    /// Original English menu label, the same strings Electron passed through
    /// `translateMain` defaults: `tray.openMCode`, `menu.settings`,
    /// `menu.checkForUpdates`, `tray.quit`.
    pub fn default_label(self) -> &'static str {
        match self {
            Self::ShowMainWindow => "Open MCode",
            Self::OpenSettings => "Settings",
            Self::CheckForUpdates => "Check for Updates...",
            Self::QuitRequested => "Quit",
        }
    }

    /// Parses a menu item id back into its event.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "show_main_window" => Some(Self::ShowMainWindow),
            "open_settings" => Some(Self::OpenSettings),
            "check_for_updates" => Some(Self::CheckForUpdates),
            "quit_requested" => Some(Self::QuitRequested),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_item_ids_are_stable() {
        // The ids double as native menu item ids; renaming one would break
        // event mapping and any persisted references.
        assert_eq!(TrayEvent::ShowMainWindow.id(), "show_main_window");
        assert_eq!(TrayEvent::OpenSettings.id(), "open_settings");
        assert_eq!(TrayEvent::CheckForUpdates.id(), "check_for_updates");
        assert_eq!(TrayEvent::QuitRequested.id(), "quit_requested");
    }

    #[test]
    fn every_menu_item_maps_back_to_its_event() {
        for event in [
            TrayEvent::ShowMainWindow,
            TrayEvent::OpenSettings,
            TrayEvent::CheckForUpdates,
            TrayEvent::QuitRequested,
        ] {
            assert_eq!(TrayEvent::from_id(event.id()), Some(event));
        }
        assert_eq!(TrayEvent::from_id("unknown"), None);
        assert_eq!(TrayEvent::from_id(""), None);
    }

    #[test]
    fn labels_match_original_menu_strings() {
        assert_eq!(TrayEvent::ShowMainWindow.default_label(), "Open MCode");
        assert_eq!(TrayEvent::OpenSettings.default_label(), "Settings");
        assert_eq!(
            TrayEvent::CheckForUpdates.default_label(),
            "Check for Updates..."
        );
        assert_eq!(TrayEvent::QuitRequested.default_label(), "Quit");
    }
}
