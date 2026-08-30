use mcode_platform::{DesktopWindowPreferences, WindowBounds};
use mcode_settings::SettingsService;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

struct TempSettings(PathBuf);

impl TempSettings {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mcode-desktop-window-settings-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        Self(path)
    }

    fn service(&self) -> SettingsService {
        SettingsService::new(self.0.clone())
    }
}

impl Drop for TempSettings {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn missing_settings_have_electron_compatible_desktop_defaults() {
    let settings = TempSettings::new();

    assert_eq!(
        settings.service().desktop_window_preferences().unwrap(),
        DesktopWindowPreferences::default()
    );
}

#[test]
fn reads_bounds_maximized_and_close_to_tray_from_the_existing_root_shape() {
    let settings = TempSettings::new();
    fs::write(
        &settings.0,
        serde_json::to_vec(&json!({
            "ui": {
                "windowBounds": { "x": -1200, "y": 18, "width": 1180, "height": 760 },
                "windowMaximized": true
            },
            "settings": { "minimizeToTrayOnClose": true }
        }))
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        settings.service().desktop_window_preferences().unwrap(),
        DesktopWindowPreferences {
            bounds: Some(WindowBounds {
                x: -1200,
                y: 18,
                width: 1180,
                height: 760
            }),
            maximized: true,
            minimize_to_tray_on_close: true
        }
    );
}

#[test]
fn malformed_optional_desktop_fields_fall_back_independently() {
    let settings = TempSettings::new();
    fs::write(
        &settings.0,
        serde_json::to_vec(&json!({
            "ui": { "windowBounds": { "x": 0, "y": 0, "width": -1, "height": 700 }, "windowMaximized": "yes" },
            "settings": { "minimizeToTrayOnClose": 1 }
        }))
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        settings.service().desktop_window_preferences().unwrap(),
        DesktopWindowPreferences::default()
    );
}

#[test]
fn placement_updates_are_atomic_merges_and_maximize_retains_normal_bounds() {
    let settings = TempSettings::new();
    fs::write(
        &settings.0,
        serde_json::to_vec(&json!({
            "untouched": { "value": 7 },
            "ui": {
                "activeView": "tasks",
                "windowBounds": { "x": 10, "y": 20, "width": 1000, "height": 700 }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    settings
        .service()
        .persist_window_placement(None, true)
        .unwrap();

    let persisted: Value = serde_json::from_slice(&fs::read(&settings.0).unwrap()).unwrap();
    assert_eq!(persisted["untouched"], json!({ "value": 7 }));
    assert_eq!(persisted["ui"]["activeView"], "tasks");
    assert_eq!(
        persisted["ui"]["windowBounds"],
        json!({ "x": 10, "y": 20, "width": 1000, "height": 700 })
    );
    assert_eq!(persisted["ui"]["windowMaximized"], true);
}

#[test]
fn close_to_tray_setting_updates_without_clobbering_other_settings() {
    let settings = TempSettings::new();
    fs::write(
        &settings.0,
        serde_json::to_vec(&json!({
            "settings": { "theme": "dark", "minimizeToTrayOnClose": false },
            "ui": { "activeView": "terminal" }
        }))
        .unwrap(),
    )
    .unwrap();

    settings
        .service()
        .set_minimize_to_tray_on_close(true)
        .unwrap();

    let persisted: Value = serde_json::from_slice(&fs::read(&settings.0).unwrap()).unwrap();
    assert_eq!(persisted["settings"]["theme"], "dark");
    assert_eq!(persisted["settings"]["minimizeToTrayOnClose"], true);
    assert_eq!(persisted["ui"]["activeView"], "terminal");
}
