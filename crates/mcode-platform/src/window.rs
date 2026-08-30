//! Window bounds persistence and placement policy.
//!
//! Shared constants and helpers for restoring the main window to its last
//! on-screen position (ported from the Electron window-management code).

use serde::{Deserialize, Serialize};

pub const MIN_PERSISTED_WINDOW_WIDTH: u32 = 600;
pub const MIN_PERSISTED_WINDOW_HEIGHT: u32 = 400;
pub const MIN_VISIBLE_WINDOW_WIDTH: u32 = 300;
pub const MIN_VISIBLE_WINDOW_HEIGHT: u32 = 200;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DesktopWindowPreferences {
    pub bounds: Option<WindowBounds>,
    pub maximized: bool,
    pub minimize_to_tray_on_close: bool,
}

pub fn restorable_window_bounds(
    bounds: Option<WindowBounds>,
    monitors: &[MonitorBounds],
) -> Option<WindowBounds> {
    let bounds = bounds?;
    if bounds.width <= MIN_PERSISTED_WINDOW_WIDTH || bounds.height <= MIN_PERSISTED_WINDOW_HEIGHT {
        return None;
    }
    monitors
        .iter()
        .any(|monitor| has_meaningful_overlap(bounds, *monitor))
        .then_some(bounds)
}

pub fn bounds_to_persist(bounds: WindowBounds, maximized: bool) -> Option<WindowBounds> {
    (!maximized
        && bounds.width > MIN_PERSISTED_WINDOW_WIDTH
        && bounds.height > MIN_PERSISTED_WINDOW_HEIGHT)
        .then_some(bounds)
}

pub fn should_hide_close_to_tray(
    is_windows: bool,
    tray_available: bool,
    preference_enabled: bool,
    shutdown_in_progress: bool,
) -> bool {
    is_windows && tray_available && preference_enabled && !shutdown_in_progress
}

fn has_meaningful_overlap(bounds: WindowBounds, monitor: MonitorBounds) -> bool {
    let left = i64::from(bounds.x).max(i64::from(monitor.x));
    let top = i64::from(bounds.y).max(i64::from(monitor.y));
    let right = (i64::from(bounds.x) + i64::from(bounds.width))
        .min(i64::from(monitor.x) + i64::from(monitor.width));
    let bottom = (i64::from(bounds.y) + i64::from(bounds.height))
        .min(i64::from(monitor.y) + i64::from(monitor.height));

    right.saturating_sub(left) >= i64::from(MIN_VISIBLE_WINDOW_WIDTH)
        && bottom.saturating_sub(top) >= i64::from(MIN_VISIBLE_WINDOW_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: MonitorBounds = MonitorBounds {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };

    #[test]
    fn restores_bounds_with_meaningful_visible_area() {
        let bounds = WindowBounds {
            x: 1620,
            y: 840,
            width: 900,
            height: 700,
        };

        assert_eq!(
            restorable_window_bounds(Some(bounds), &[PRIMARY]),
            Some(bounds)
        );
    }

    #[test]
    fn rejects_offscreen_and_one_pixel_sliver_bounds() {
        let offscreen = WindowBounds {
            x: 2000,
            y: 0,
            width: 900,
            height: 700,
        };
        let sliver = WindowBounds {
            x: 1919,
            y: 0,
            width: 900,
            height: 700,
        };

        assert_eq!(restorable_window_bounds(Some(offscreen), &[PRIMARY]), None);
        assert_eq!(restorable_window_bounds(Some(sliver), &[PRIMARY]), None);
    }

    #[test]
    fn rejects_bounds_at_or_below_the_electron_save_threshold() {
        let too_narrow = WindowBounds {
            x: 0,
            y: 0,
            width: MIN_PERSISTED_WINDOW_WIDTH,
            height: 700,
        };
        let too_short = WindowBounds {
            x: 0,
            y: 0,
            width: 900,
            height: MIN_PERSISTED_WINDOW_HEIGHT,
        };

        assert_eq!(restorable_window_bounds(Some(too_narrow), &[PRIMARY]), None);
        assert_eq!(restorable_window_bounds(Some(too_short), &[PRIMARY]), None);
    }

    #[test]
    fn does_not_persist_maximized_or_near_minimum_bounds() {
        let normal = WindowBounds {
            x: 12,
            y: 24,
            width: 1200,
            height: 800,
        };
        let near_minimum = WindowBounds {
            width: MIN_PERSISTED_WINDOW_WIDTH,
            ..normal
        };

        assert_eq!(bounds_to_persist(normal, false), Some(normal));
        assert_eq!(bounds_to_persist(normal, true), None);
        assert_eq!(bounds_to_persist(near_minimum, false), None);
    }

    #[test]
    fn close_to_tray_is_windows_only_and_never_intercepts_shutdown() {
        assert!(should_hide_close_to_tray(true, true, true, false));
        assert!(!should_hide_close_to_tray(false, true, true, false));
        assert!(!should_hide_close_to_tray(true, false, true, false));
        assert!(!should_hide_close_to_tray(true, true, false, false));
        assert!(!should_hide_close_to_tray(true, true, true, true));
    }
}
