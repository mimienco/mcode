use tauri::{AppHandle, Runtime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayInstallStatus {
    Installed,
    InProgress,
}

#[cfg(target_os = "windows")]
mod windows {
    use std::io;
    use std::sync::atomic::{AtomicU8, Ordering};

    use tauri::{
        AppHandle, Manager, Runtime,
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    const MAIN_WINDOW_LABEL: &str = "main";
    const OPEN_MENU_ID: &str = "windows-tray-open";
    const QUIT_MENU_ID: &str = "windows-tray-quit";
    const SETTINGS_MENU_ID: &str = "windows-tray-settings";
    const NOT_INSTALLED: u8 = 0;
    const INSTALLING: u8 = 1;
    const INSTALLED: u8 = 2;
    static INSTALLATION_STATE: AtomicU8 = AtomicU8::new(NOT_INSTALLED);

    pub(super) fn install<R: Runtime>(
        app: &AppHandle<R>,
    ) -> tauri::Result<super::TrayInstallStatus> {
        loop {
            match INSTALLATION_STATE.load(Ordering::Acquire) {
                INSTALLED => return Ok(super::TrayInstallStatus::Installed),
                INSTALLING => return Ok(super::TrayInstallStatus::InProgress),
                NOT_INSTALLED => {
                    if INSTALLATION_STATE
                        .compare_exchange(
                            NOT_INSTALLED,
                            INSTALLING,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        break;
                    }
                }
                _ => unreachable!("invalid tray installation state"),
            }
        }
        if let Err(error) = install_claimed(app) {
            INSTALLATION_STATE.store(NOT_INSTALLED, Ordering::Release);
            return Err(error);
        }
        INSTALLATION_STATE.store(INSTALLED, Ordering::Release);
        Ok(super::TrayInstallStatus::Installed)
    }

    fn install_claimed<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
        let open = MenuItem::with_id(app, OPEN_MENU_ID, "Open MCode", true, None::<&str>)?;
        let settings = MenuItem::with_id(app, SETTINGS_MENU_ID, "Settings…", true, None::<&str>)?;
        let separator = PredefinedMenuItem::separator(app)?;
        let quit = MenuItem::with_id(app, QUIT_MENU_ID, "Quit", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &settings, &separator, &quit])?;
        let icon = app
            .default_window_icon()
            .cloned()
            .ok_or_else(|| io::Error::other("MCode has no default window icon"))?;

        TrayIconBuilder::new()
            .icon(icon)
            .tooltip("MCode")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id() {
                id if id == OPEN_MENU_ID => restore_main_window(app),
                // Settings must both reveal the window and push the intent:
                // unlike Open, the renderer needs the ui:openSettings event
                // (plus the pending one-shot for a cold renderer) to actually
                // navigate to the Settings surface.
                id if id == SETTINGS_MENU_ID => {
                    let _ = crate::app_menu_events::open_settings(app);
                }
                id if id == QUIT_MENU_ID => {
                    restore_main_window(app);
                    app.exit(0);
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                ) {
                    restore_main_window(tray.app_handle());
                }
            })
            .build(app)?;

        Ok(())
    }

    fn restore_main_window<R: Runtime>(app: &AppHandle<R>) {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayInstallStatus> {
    #[cfg(target_os = "windows")]
    return windows::install(app);

    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Ok(TrayInstallStatus::Installed)
    }
}
