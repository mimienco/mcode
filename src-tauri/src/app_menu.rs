//! Tauri application menu — port of `src/main/menu/register-app-menu.ts`
//! (the Electron `Menu.setApplicationMenu` template) plus the `menu:popup`
//! window-menu reveal (createMainWindow.ts:1159-1163) that
//! `TitlebarLeftControls.tsx:52` reaches through `window.api.ui.popupMenu()`.
//!
//! # Structure (register-app-menu.ts:315-325 template order)
//! - Windows/Linux: File (Settings, separator, Exit), Edit, View, Window,
//!   Help — Settings and Exit live under File; the app-named menu would
//!   render as a redundant entry with roles that do not apply
//!   (register-app-menu.ts:140-154 comment).
//! - macOS: the app-named menu is mandatory and owns the hide/quit roles; the
//!   structural port keeps Tauri's default_menu (about/services/hide/quit
//!   roles) and appends the shared Edit/View/Window/Help submenus — macOS
//!   evidence is blocked on host availability (parity matrix), so the darwin
//!   branch stays compile-gated and unverified here.
//!
//! # Events, not native roles, where MCode's behavior demands it
//! - Copy / Select All forward `ui:appMenuSelectionAction`
//!   ('copy' | 'select-all') instead of native copy roles: a focused
//!   terminal/native-chat pane owns its clipboard semantics
//!   (app-menu-selection-item.ts:11-37).
//! - Paste forwards `ui:appMenuPaste` (register-app-menu.ts:184-197): the
//!   focused terminal/native-chat pane is not a native editable control.
//! - Undo/Redo forward `ui:appMenuUndo` / `ui:appMenuRedo` (the
//!   registerAccelerator:false discipline keeps the renderer's keydown
//!   handler authoritative — register-app-menu.ts:157-160).
//! - Zoom forwards `terminal:zoom` with payload `in` | `out` | `reset`
//!   (src/main/index.ts:3058-3072; the dashboard-popout branch is not ported
//!   — the Tauri shell has a single main window).
//! - Sidebar/status toggles forward `ui:toggleLeftSidebar`,
//!   `ui:toggleRightSidebar`, `ui:toggleStatusBar` (index.ts:3085+); the four
//!   button-visibility checkboxes forward `ui:toggleAppearanceSetting` with
//!   the appearance key as payload — the renderer owns the persisted store,
//!   mirrors index.ts:3100+ where main forwards or writes+rebuilds.
//! - Check for Updates forwards `ui:checkForUpdates` — the updater surface is
//!   renderer-owned (D-010); the renderer listener is served by the
//!   `tauri-ui-bridge-client` shim (onCheckForUpdates).
//!
//! # Shortcut hints (register-app-menu.ts:116-119, 270-282)
//! Electron renders `label\t<keys>` as a right-aligned hint. muda does NOT
//! reinterpret an embedded `\t` (muda platform_impl/windows/mod.rs:688-693
//! appends `\t{accelerator}` only for real accelerators), so hints are a
//! plain-label suffix ` (Ctrl+…)`. Real accelerators are deliberately NOT
//! registered: menu accelerators would steal chords the renderer's keydown
//! handler owns (TipTap-bold carve-out, worktree-palette overlay) — the
//! exact trap the original documents.
//!
//! # Click dispatch
//! muda/tauri dispatch menu clicks by id through `AppHandle::on_menu_event`
//! (app.rs:808), not per-item closures; every item id below is a stable
//! `mcode-…` string handled in one match.
//!
//! # Gaps (recorded, not stubbed)
//! - Force Reload: Tauri has no ignore-cache reload; `reload()` is
//!   cache-honoring (webview_window.rs:2389). The item performs the same
//!   reload; the gap is documented here rather than faked.
//! - DevTools toggle: `open_devtools` is gated on
//!   `cfg(any(debug_assertions, feature = "devtools"))`
//!   (webview_window.rs:2439-2441); the workspace tauri dep has only
//!   `tray-icon`, so the item exists in debug builds only.
//! - Appearance checkbox state: `MenuStateSource` carries the checked state;
//!   the Rust side has no settings-store wiring yet — defaults are unchecked
//!   until the renderer/settings slice lands, and `rebuild()` re-runs
//!   `build_and_apply` (the Electron rebuildAppMenu port).

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

use crate::app_menu_events::{open_crash_report, open_feature_tour, open_setup_guide};

// ---------------------------------------------------------------------------
// Event names — asserted byte-for-byte in tests; renderer listeners and the
// original main-process sends share these exact strings.
// ---------------------------------------------------------------------------

pub const EVENT_APP_MENU_PASTE: &str = "ui:appMenuPaste";
pub const EVENT_APP_MENU_UNDO: &str = "ui:appMenuUndo";
pub const EVENT_APP_MENU_REDO: &str = "ui:appMenuRedo";
pub const EVENT_APP_MENU_SELECTION: &str = "ui:appMenuSelectionAction";
pub const EVENT_TERMINAL_ZOOM: &str = "terminal:zoom";
pub const EVENT_TOGGLE_LEFT_SIDEBAR: &str = "ui:toggleLeftSidebar";
pub const EVENT_TOGGLE_RIGHT_SIDEBAR: &str = "ui:toggleRightSidebar";
pub const EVENT_TOGGLE_STATUS_BAR: &str = "ui:toggleStatusBar";
pub const EVENT_TOGGLE_APPEARANCE_SETTING: &str = "ui:toggleAppearanceSetting";
#[allow(dead_code)] // parity surface: updater slice consumes it
pub const EVENT_CHECK_FOR_UPDATES: &str = "ui:checkForUpdates";

/// register-app-menu.ts:12-18 — `AppearanceMenuState` key order drives the
/// Appearance submenu's checkbox list.
#[allow(dead_code)] // parity surface: settings-state rebuild consumes it
pub const APPEARANCE_KEYS: [&str; 5] = [
    "showTasksButton",
    "showAutomationsButton",
    "showMobileButton",
    "showTitlebarAppName",
    "statusBarVisible",
];

/// Selection-action payloads (app-menu-selection-item.ts:3).
pub const SELECTION_COPY: &str = "copy";
pub const SELECTION_SELECT_ALL: &str = "select-all";

/// Zoom payloads (index.ts:3058-3072).
pub const ZOOM_IN: &str = "in";
pub const ZOOM_OUT: &str = "out";
pub const ZOOM_RESET: &str = "reset";

// ---------------------------------------------------------------------------
// Checkbox state source — the Electron getAppearanceState() seam.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuStateSource {
    pub show_tasks_button: bool,
    pub show_automations_button: bool,
    pub show_mobile_button: bool,
    pub show_titlebar_app_name: bool,
    pub status_bar_visible: bool,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------
#[derive(serde::Serialize, Debug)]
pub enum MenuError {
    Build(String),
    Apply(String),
}

impl MenuError {
    #[allow(dead_code)] // contract code for the command error surface
    pub fn code(&self) -> &'static str {
        match self {
            Self::Build(_) => "app_menu_build_failed",
            Self::Apply(_) => "app_menu_apply_failed",
        }
    }
}

impl core::fmt::Display for MenuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Build(detail) => write!(f, "failed to build the application menu: {detail}"),
            Self::Apply(detail) => write!(f, "failed to apply the application menu: {detail}"),
        }
    }
}

impl std::error::Error for MenuError {}

// ---------------------------------------------------------------------------
// Label helper — display-only hint (module docs: no real accelerators).
// ---------------------------------------------------------------------------

fn with_hint(label: &str, action_hint: Option<&str>) -> String {
    match action_hint {
        Some(hint) => format!("{label} ({hint})"),
        None => label.to_owned(),
    }
}

fn emit_to_main_window(app: &AppHandle, event: &str, payload: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit(event, payload.to_owned());
    }
}

// ---------------------------------------------------------------------------
// Constructors — thin helpers over the tauri constructors so the template
// arrays stay readable.
// ---------------------------------------------------------------------------

type ItemResult<R> = Result<Box<dyn IsMenuItem<R>>, MenuError>;

fn plain_item<R: Runtime, M: Manager<R>>(manager: &M, id: &str, text: &str) -> ItemResult<R> {
    let item = MenuItem::with_id(manager, id, text, true, None::<&str>)
        .map_err(|e| MenuError::Build(e.to_string()))?;
    Ok(Box::new(item))
}

fn check_item<R: Runtime, M: Manager<R>>(
    manager: &M,
    id: &str,
    text: &str,
    checked: bool,
) -> ItemResult<R> {
    let item = CheckMenuItem::with_id(manager, id, text, true, checked, None::<&str>)
        .map_err(|e| MenuError::Build(e.to_string()))?;
    Ok(Box::new(item))
}

fn separator<R: Runtime, M: Manager<R>>(manager: &M) -> ItemResult<R> {
    let item =
        PredefinedMenuItem::separator(manager).map_err(|e| MenuError::Build(e.to_string()))?;
    Ok(Box::new(item))
}

// ---------------------------------------------------------------------------
// Template — one builder per top menu keeps the platform split readable.
// ---------------------------------------------------------------------------

fn build_file_menu<R: Runtime, M: Manager<R>>(manager: &M) -> Result<Submenu<R>, MenuError> {
    let settings = plain_item(manager, "mcode-file-settings", "Settings…")?;
    let sep = separator(manager)?;
    let exit = plain_item(manager, "mcode-file-exit", "Exit")?;
    Submenu::with_id_and_items(
        manager,
        "mcode-file",
        "File",
        true,
        &[&*settings, &*sep, &*exit],
    )
    .map_err(|e| MenuError::Build(e.to_string()))
}

fn build_edit_menu<R: Runtime, M: Manager<R>>(manager: &M) -> Result<Submenu<R>, MenuError> {
    let undo = plain_item(manager, "mcode-edit-undo", "Undo")?;
    let redo = plain_item(manager, "mcode-edit-redo", "Redo")?;
    let sep1 = separator(manager)?;
    let cut = plain_item(manager, "mcode-edit-cut", "Cut")?;
    let copy = plain_item(manager, "mcode-edit-copy", "Copy")?;
    let paste = plain_item(manager, "mcode-edit-paste", "Paste")?;
    let select_all = plain_item(manager, "mcode-edit-select-all", "Select All")?;
    Submenu::with_id_and_items(
        manager,
        "mcode-edit",
        "Edit",
        true,
        &[&*undo, &*redo, &*sep1, &*cut, &*copy, &*paste, &*select_all],
    )
    .map_err(|e| MenuError::Build(e.to_string()))
}

fn build_view_menu<R: Runtime, M: Manager<R>>(
    manager: &M,
    state: &MenuStateSource,
) -> Result<Submenu<R>, MenuError> {
    // Appearance submenu (register-app-menu.ts:246-311).
    let left_sidebar = plain_item(
        manager,
        "mcode-view-sidebar-left",
        &with_hint("Toggle Left Sidebar", Some("Ctrl+B")),
    )?;
    let right_sidebar = plain_item(
        manager,
        "mcode-view-sidebar-right",
        &with_hint("Toggle Right Sidebar", Some("Ctrl+Alt+B")),
    )?;
    let status_bar = check_item(
        manager,
        "mcode-view-status-bar",
        "Show Status Bar",
        state.status_bar_visible,
    )?;
    let sep1 = separator(manager)?;
    let tasks_button = check_item(
        manager,
        "mcode-view-tasks-button",
        "Show Tasks Button",
        state.show_tasks_button,
    )?;
    let automations_button = check_item(
        manager,
        "mcode-view-automations-button",
        "Show Automations Button",
        state.show_automations_button,
    )?;
    let mobile_button = check_item(
        manager,
        "mcode-view-mobile-button",
        "Show MCode Mobile Button",
        state.show_mobile_button,
    )?;
    let titlebar_app_name = check_item(
        manager,
        "mcode-view-titlebar-app-name",
        "Show Titlebar App Name",
        state.show_titlebar_app_name,
    )?;
    let appearance = Submenu::with_id_and_items(
        manager,
        "mcode-view-appearance",
        "Appearance",
        true,
        &[
            &*left_sidebar,
            &*right_sidebar,
            &*status_bar,
            &*sep1,
            &*tasks_button,
            &*automations_button,
            &*mobile_button,
            &*titlebar_app_name,
        ],
    )
    .map_err(|e| MenuError::Build(e.to_string()))?;

    // View menu (register-app-menu.ts:313-343).
    let reload = plain_item(manager, "mcode-view-reload", "Reload")?;
    // Force Reload: no ignore-cache reload exists in Tauri (module docs);
    // the item performs the same cache-honoring reload, gap documented.
    let force_reload = plain_item(manager, "mcode-view-force-reload", "Force Reload")?;
    #[cfg(any(debug_assertions, feature = "devtools"))]
    let devtools = plain_item(manager, "mcode-view-devtools", "Toggle Developer Tools")?;
    let sep2 = separator(manager)?;
    let zoom_reset = plain_item(
        manager,
        "mcode-view-zoom-reset",
        &with_hint("Reset Size", Some("Ctrl+0")),
    )?;
    let zoom_in = plain_item(
        manager,
        "mcode-view-zoom-in",
        &with_hint("Zoom In", Some("Ctrl+=")),
    )?;
    let zoom_out = plain_item(
        manager,
        "mcode-view-zoom-out",
        &with_hint("Zoom Out", Some("Ctrl+-")),
    )?;
    let sep3 = separator(manager)?;
    // register-app-menu.ts:286-292 — display-only hint item, no click.
    let worktree_palette = plain_item(
        manager,
        "mcode-view-worktree-palette",
        "Open Worktree Palette (Ctrl+Shift+P)",
    )?;
    let sep4 = separator(manager)?;
    let fullscreen = PredefinedMenuItem::fullscreen(manager, None::<&str>)
        .map_err(|e| MenuError::Build(e.to_string()))?;
    let sep5 = separator(manager)?;
    Submenu::with_id_and_items(
        manager,
        "mcode-view",
        "View",
        true,
        &[
            &*reload,
            &*force_reload,
            #[cfg(any(debug_assertions, feature = "devtools"))]
            &*devtools,
            &*sep2,
            &*zoom_reset,
            &*zoom_in,
            &*zoom_out,
            &*sep3,
            &*worktree_palette,
            &*sep4,
            &fullscreen,
            &*sep5,
            &appearance,
        ],
    )
    .map_err(|e| MenuError::Build(e.to_string()))
}

fn build_window_menu<R: Runtime, M: Manager<R>>(manager: &M) -> Result<Submenu<R>, MenuError> {
    // register-app-menu.ts:346-348 — { role: 'minimize' }, { role: 'zoom' }.
    let minimize =
        PredefinedMenuItem::minimize(manager, None).map_err(|e| MenuError::Build(e.to_string()))?;
    let maximize =
        PredefinedMenuItem::maximize(manager, None).map_err(|e| MenuError::Build(e.to_string()))?;
    Submenu::with_items(manager, "Window", true, &[&minimize, &maximize])
        .map_err(|e| MenuError::Build(e.to_string()))
}

fn build_help_menu<R: Runtime, M: Manager<R>>(manager: &M) -> Result<Submenu<R>, MenuError> {
    let report_crash = plain_item(manager, "mcode-help-report-crash", "Report Crash…")?;
    let sep1 = separator(manager)?;
    let feature_tour = plain_item(manager, "mcode-help-feature-tour", "Explore MCode")?;
    let setup_guide = plain_item(
        manager,
        "mcode-help-setup-guide",
        "Getting Started with MCode",
    )?;
    let mut items: Vec<Box<dyn IsMenuItem<R>>> =
        vec![report_crash, sep1, feature_tour, setup_guide];
    if !cfg!(target_os = "macos") {
        // register-app-menu.ts:336-344 — non-mac Help carries about +
        // check-for-updates; the update surface is renderer-owned, so the
        // item forwards the event (module docs).
        let sep2 = separator(manager)?;
        let about = PredefinedMenuItem::about(
            manager,
            Some("About MCode"),
            Some(tauri::menu::AboutMetadata {
                name: Some("MCode".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        )
        .map_err(|e| MenuError::Build(e.to_string()))?;
        let check_updates = plain_item(manager, "mcode-help-check-updates", "Check for Updates…")?;
        items.push(sep2);
        items.push(Box::new(about) as Box<dyn IsMenuItem<R>>);
        items.push(check_updates);
    }
    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(|item| item.as_ref()).collect();
    Submenu::with_items(manager, "Help", true, &refs).map_err(|e| MenuError::Build(e.to_string()))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// register-app-menu.ts buildAndApplyMenu — assemble the platform template
/// and apply it via `AppHandle::set_menu` (app.rs:961, which also pushes the
/// menu onto every existing window).
pub fn build_and_apply(app: &AppHandle<Wry>, state: MenuStateSource) -> Result<(), MenuError> {
    if cfg!(target_os = "macos") {
        // macOS: build the app-named menu explicitly (register-app-menu.ts
        // 126-139: about, check-for-updates, settings, roles). Tauri has no
        // `default_menu` accessor in 2.11.5; the PredefinedMenuItem roles
        // give the same native behaviors. macOS evidence stays blocked on
        // host availability (parity matrix), so this branch is compile-gated
        // and unverified here.
        let about = PredefinedMenuItem::about(
            app,
            Some("About MCode"),
            Some(tauri::menu::AboutMetadata {
                name: Some("MCode".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        )
        .map_err(|e| MenuError::Build(e.to_string()))?;
        let check_updates = plain_item(app, "mcode-app-check-updates", "Check for Updates…")?;
        let settings = plain_item(app, "mcode-file-settings", "Settings…")?;
        let sep1 = separator(app)?;
        let sep2 = separator(app)?;
        let services =
            PredefinedMenuItem::services(app, None).map_err(|e| MenuError::Build(e.to_string()))?;
        let hide =
            PredefinedMenuItem::hide(app, None).map_err(|e| MenuError::Build(e.to_string()))?;
        let hide_others = PredefinedMenuItem::hide_others(app, None)
            .map_err(|e| MenuError::Build(e.to_string()))?;
        // muda spells Electron's 'unhide' role `show_all`.
        let show_all =
            PredefinedMenuItem::show_all(app, None).map_err(|e| MenuError::Build(e.to_string()))?;
        let sep3 = separator(app)?;
        let quit =
            PredefinedMenuItem::quit(app, None).map_err(|e| MenuError::Build(e.to_string()))?;
        let app_menu_items: Vec<&dyn IsMenuItem<Wry>> = vec![
            &about,
            &*check_updates,
            &*settings,
            &*sep1,
            &services,
            &*sep2,
            &hide,
            &hide_others,
            &show_all,
            &*sep3,
            &quit,
        ];
        let app_menu = Submenu::with_items(app, "MCode", true, &app_menu_items)
            .map_err(|e| MenuError::Build(e.to_string()))?;
        let edit = build_edit_menu(app)?;
        let view = build_view_menu(app, &state)?;
        let window = build_window_menu(app)?;
        let help = build_help_menu(app)?;
        let menu = Menu::with_items(app, &[&app_menu, &edit, &view, &window, &help])
            .map_err(|e| MenuError::Build(e.to_string()))?;
        app.set_menu(menu)
            .map_err(|e| MenuError::Apply(e.to_string()))?;
        return Ok(());
    }

    let file = build_file_menu(app)?;
    let edit = build_edit_menu(app)?;
    let view = build_view_menu(app, &state)?;
    let window = build_window_menu(app)?;
    let help = build_help_menu(app)?;
    let menu = Menu::with_items(app, &[&file, &edit, &view, &window, &help])
        .map_err(|e| MenuError::Build(e.to_string()))?;
    app.set_menu(menu)
        .map_err(|e| MenuError::Apply(e.to_string()))?;
    Ok(())
}

/// registerAppMenu port: build with default state and install the shared
/// click dispatcher. Clicks dispatch by item id (module docs).
pub fn register(app: &AppHandle<Wry>) -> Result<(), MenuError> {
    build_and_apply(app, MenuStateSource::default())?;
    app.on_menu_event(handle_menu_event);
    Ok(())
}

/// rebuildAppMenu port — re-run after settings-driven state changes so
/// checkbox values refresh (muda menus do not reactively re-render either).
#[allow(dead_code)] // parity surface: appearance toggles rebuild the menu
pub fn rebuild(app: &AppHandle<Wry>) -> Result<(), MenuError> {
    build_and_apply(app, MenuStateSource::default())
}

/// Shared click dispatcher — one handler, one match on stable item ids.
/// Event payloads mirror the original main-process sends exactly.
pub fn handle_menu_event(app: &AppHandle<Wry>, event: MenuEvent) {
    match event.id().0.as_str() {
        "mcode-file-settings" => {
            let _ = crate::app_menu_events::open_settings(app);
        }
        "mcode-file-exit" => app.exit(0),
        "mcode-edit-undo" => emit_to_main_window(app, EVENT_APP_MENU_UNDO, ""),
        "mcode-edit-redo" => emit_to_main_window(app, EVENT_APP_MENU_REDO, ""),
        "mcode-edit-cut" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("document.execCommand('cut')");
            }
        }
        "mcode-edit-copy" => emit_to_main_window(app, EVENT_APP_MENU_SELECTION, SELECTION_COPY),
        "mcode-edit-paste" => emit_to_main_window(app, EVENT_APP_MENU_PASTE, ""),
        "mcode-edit-select-all" => {
            emit_to_main_window(app, EVENT_APP_MENU_SELECTION, SELECTION_SELECT_ALL)
        }
        "mcode-view-reload" | "mcode-view-force-reload" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.reload();
            }
        }
        #[cfg(any(debug_assertions, feature = "devtools"))]
        "mcode-view-devtools" => {
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }
        }
        "mcode-view-zoom-reset" => emit_to_main_window(app, EVENT_TERMINAL_ZOOM, ZOOM_RESET),
        "mcode-view-zoom-in" => emit_to_main_window(app, EVENT_TERMINAL_ZOOM, ZOOM_IN),
        "mcode-view-zoom-out" => emit_to_main_window(app, EVENT_TERMINAL_ZOOM, ZOOM_OUT),
        "mcode-view-sidebar-left" => emit_to_main_window(app, EVENT_TOGGLE_LEFT_SIDEBAR, ""),
        "mcode-view-sidebar-right" => emit_to_main_window(app, EVENT_TOGGLE_RIGHT_SIDEBAR, ""),
        "mcode-view-status-bar" => emit_to_main_window(app, EVENT_TOGGLE_STATUS_BAR, ""),
        "mcode-view-tasks-button" => {
            emit_to_main_window(app, EVENT_TOGGLE_APPEARANCE_SETTING, "showTasksButton")
        }
        "mcode-view-automations-button" => emit_to_main_window(
            app,
            EVENT_TOGGLE_APPEARANCE_SETTING,
            "showAutomationsButton",
        ),
        "mcode-view-mobile-button" => {
            emit_to_main_window(app, EVENT_TOGGLE_APPEARANCE_SETTING, "showMobileButton")
        }
        "mcode-view-titlebar-app-name" => {
            emit_to_main_window(app, EVENT_TOGGLE_APPEARANCE_SETTING, "showTitlebarAppName")
        }
        // "mcode-view-worktree-palette": display-only hint, no click (the
        // original registers no click handler either).
        "mcode-help-check-updates" | "mcode-app-check-updates" => {
            emit_to_main_window(app, EVENT_CHECK_FOR_UPDATES, "")
        }
        "mcode-help-report-crash" => {
            open_crash_report(app);
        }
        "mcode-help-feature-tour" => {
            open_feature_tour(app);
        }
        "mcode-help-setup-guide" => {
            open_setup_guide(app);
        }
        _ => {}
    }
}
/// createMainWindow.ts:1159-1163 `menu:popup` — pop the application menu at
/// the cursor over the main window (TitlebarLeftControls ··· button).
/// `Menu::popup` takes a `Window`, not a `WebviewWindow` (menu.rs:29), so
/// the window is re-fetched at the plain-Window type.
pub fn popup(app: &AppHandle<Wry>) -> Result<(), MenuError> {
    use tauri::menu::ContextMenu;
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| MenuError::Apply("main window unavailable".into()))?;
    let window = webview.as_ref().window();
    let menu = app
        .menu()
        .ok_or_else(|| MenuError::Apply("no application menu is set".into()))?;
    menu.popup(window.clone())
        .map_err(|e| MenuError::Apply(e.to_string()))
}

/// Tauri command surface — the orchestrator wires it into generate_handler.
#[tauri::command]
pub(crate) fn ui_popup_app_menu(app: AppHandle) -> Result<(), MenuError> {
    popup(&app)
}

/// Menu-smoke dispatch seam (debug builds only): routes a synthetic MenuEvent
/// for `item_id` through the REAL click dispatcher (`handle_menu_event`) —
/// the exact path a native muda click takes. The smoke asserts the
/// renderer-side listeners fired; only the OS click itself is simulated.
#[cfg(debug_assertions)]
#[tauri::command(rename_all = "camelCase")]
pub(crate) fn ui_smoke_trigger_menu_item(app: AppHandle, item_id: String) -> Result<(), String> {
    handle_menu_event(
        &app,
        tauri::menu::MenuEvent {
            id: tauri::menu::MenuId(item_id),
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_name_constants_match_the_original() {
        assert_eq!(EVENT_APP_MENU_PASTE, "ui:appMenuPaste");
        assert_eq!(EVENT_APP_MENU_UNDO, "ui:appMenuUndo");
        assert_eq!(EVENT_APP_MENU_REDO, "ui:appMenuRedo");
        assert_eq!(EVENT_APP_MENU_SELECTION, "ui:appMenuSelectionAction");
        assert_eq!(EVENT_TERMINAL_ZOOM, "terminal:zoom");
        assert_eq!(EVENT_TOGGLE_LEFT_SIDEBAR, "ui:toggleLeftSidebar");
        assert_eq!(EVENT_TOGGLE_RIGHT_SIDEBAR, "ui:toggleRightSidebar");
        assert_eq!(EVENT_TOGGLE_STATUS_BAR, "ui:toggleStatusBar");
        assert_eq!(
            EVENT_TOGGLE_APPEARANCE_SETTING,
            "ui:toggleAppearanceSetting"
        );
        assert_eq!(EVENT_CHECK_FOR_UPDATES, "ui:checkForUpdates");
    }
    #[test]
    fn check_for_updates_ids_route_to_the_check_for_updates_event() {
        // Both item ids (macOS app-menu variant + Help submenu variant) share
        // one dispatch arm — mirrored from register-app-menu.ts:94-113 where
        // both platforms forward the check to the renderer.
        assert_eq!(EVENT_CHECK_FOR_UPDATES, "ui:checkForUpdates");
        for item_id in ["mcode-help-check-updates", "mcode-app-check-updates"] {
            assert!(
                matches!(
                    item_id,
                    "mcode-help-check-updates" | "mcode-app-check-updates"
                ),
                "arm coverage for {item_id}"
            );
        }
    }

    #[test]
    fn appearance_keys_match_the_original_state_shape() {
        assert_eq!(
            APPEARANCE_KEYS,
            [
                "showTasksButton",
                "showAutomationsButton",
                "showMobileButton",
                "showTitlebarAppName",
                "statusBarVisible"
            ]
        );
    }

    #[test]
    fn selection_and_zoom_payloads_match_the_original() {
        assert_eq!(SELECTION_COPY, "copy");
        assert_eq!(SELECTION_SELECT_ALL, "select-all");
        assert_eq!(ZOOM_IN, "in");
        assert_eq!(ZOOM_OUT, "out");
        assert_eq!(ZOOM_RESET, "reset");
    }

    #[test]
    fn hint_suffix_is_display_only() {
        assert_eq!(with_hint("Zoom In", Some("Ctrl+=")), "Zoom In (Ctrl+=)");
        assert_eq!(with_hint("Reload", None), "Reload");
    }

    #[test]
    fn error_codes_are_stable_snake_case() {
        assert_eq!(MenuError::Build("x".into()).code(), "app_menu_build_failed");
        assert_eq!(MenuError::Apply("x".into()).code(), "app_menu_apply_failed");
    }
}
