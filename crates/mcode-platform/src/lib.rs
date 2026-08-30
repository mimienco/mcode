//! Platform integrations for MCode (tray icon, window placement, related
//! shell surfaces).
//!
//! The tray lives here as a pure emitter: it renders the icon and menu and
//! reports user intent through [`events::TrayEvent`] callbacks. It never
//! performs app-level actions itself (no shutdown, no window management),
//! keeping the shutdown protocol entirely in `mcode-app`.

pub mod events;
pub mod tray;
pub mod window;
pub mod wsl;

pub use events::TrayEvent;
pub use tray::{TRAY_ICON_PATH, TRAY_TOOLTIP, TrayError, install_tray};
pub use window::{
    DesktopWindowPreferences, MIN_PERSISTED_WINDOW_HEIGHT, MIN_PERSISTED_WINDOW_WIDTH,
    MIN_VISIBLE_WINDOW_HEIGHT, MIN_VISIBLE_WINDOW_WIDTH, MonitorBounds, WindowBounds,
    bounds_to_persist, restorable_window_bounds, should_hide_close_to_tray,
};
pub use wsl::wsl_home_directory;
