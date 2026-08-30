use crate::DesktopCommandError;
use mcode_app::LifecycleService;
use mcode_platform::{
    DesktopWindowPreferences, MonitorBounds, WindowBounds, bounds_to_persist,
    should_hide_close_to_tray,
};
use mcode_settings::{SettingsService, SettingsServiceError};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{WebviewWindow, WindowEvent};

const WINDOW_PLACEMENT_DEBOUNCE: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub(crate) struct NativeWindowState {
    inner: Arc<NativeWindowStateInner>,
}

#[derive(Debug)]
struct NativeWindowStateInner {
    settings: SettingsService,
    minimize_to_tray_on_close: AtomicBool,
    close_to_tray_revision: AtomicU64,
    tray_available: AtomicBool,
    persistence_revision: AtomicU64,
    persistence_worker_running: AtomicBool,
    persistence_frozen: AtomicBool,
    persistence_gate: Mutex<()>,
}

impl NativeWindowState {
    pub(crate) fn new(settings: SettingsService, preferences: DesktopWindowPreferences) -> Self {
        Self {
            inner: Arc::new(NativeWindowStateInner {
                settings,
                minimize_to_tray_on_close: AtomicBool::new(preferences.minimize_to_tray_on_close),
                close_to_tray_revision: AtomicU64::new(0),
                tray_available: AtomicBool::new(false),
                persistence_revision: AtomicU64::new(0),
                persistence_worker_running: AtomicBool::new(false),
                persistence_frozen: AtomicBool::new(false),
                persistence_gate: Mutex::new(()),
            }),
        }
    }

    fn minimize_to_tray_on_close(&self) -> bool {
        self.inner.minimize_to_tray_on_close.load(Ordering::Acquire)
    }

    pub(crate) fn mark_tray_available(&self) {
        self.inner.tray_available.store(true, Ordering::Release);
    }

    pub(crate) fn tray_available(&self) -> bool {
        self.inner.tray_available.load(Ordering::Acquire)
    }

    fn set_minimize_to_tray_on_close(&self, enabled: bool) -> Result<(), SettingsServiceError> {
        self.inner.settings.set_minimize_to_tray_on_close(enabled)?;
        self.inner
            .minimize_to_tray_on_close
            .store(enabled, Ordering::Release);
        self.inner
            .close_to_tray_revision
            .fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    pub(crate) fn close_to_tray_revision(&self) -> u64 {
        self.inner.close_to_tray_revision.load(Ordering::Acquire)
    }

    fn schedule_placement_persistence(&self, window: WebviewWindow) {
        if self.inner.persistence_frozen.load(Ordering::Acquire) {
            return;
        }
        self.inner
            .persistence_revision
            .fetch_add(1, Ordering::AcqRel);
        if self
            .inner
            .persistence_worker_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        let state = self.clone();
        std::thread::spawn(move || state.run_persistence_worker(window));
    }

    fn run_persistence_worker(&self, window: WebviewWindow) {
        loop {
            let observed_revision = self.inner.persistence_revision.load(Ordering::Acquire);
            std::thread::sleep(WINDOW_PLACEMENT_DEBOUNCE);
            if self.inner.persistence_frozen.load(Ordering::Acquire) {
                self.inner
                    .persistence_worker_running
                    .store(false, Ordering::Release);
                return;
            }
            if self.inner.persistence_revision.load(Ordering::Acquire) != observed_revision {
                continue;
            }

            if let Err(error) = self.persist_current_placement(&window) {
                eprintln!("[window] failed to persist placement: {error}");
            }
            self.inner
                .persistence_worker_running
                .store(false, Ordering::Release);

            if self.inner.persistence_revision.load(Ordering::Acquire) == observed_revision
                || self.inner.persistence_frozen.load(Ordering::Acquire)
                || self
                    .inner
                    .persistence_worker_running
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                return;
            }
        }
    }

    fn persist_current_placement(&self, window: &WebviewWindow) -> Result<(), String> {
        if window.is_fullscreen().map_err(|error| error.to_string())? {
            return Ok(());
        }
        let maximized = window.is_maximized().map_err(|error| error.to_string())?;
        let bounds = if maximized {
            None
        } else {
            bounds_to_persist(window_bounds(window)?, false)
        };
        // Tauri window getters synchronously consult the UI event loop. Capture them
        // before taking the persistence gate so ExitRequested can never deadlock the
        // UI thread while this worker waits for another window getter response.
        let _persistence_guard = self
            .inner
            .persistence_gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.inner.persistence_frozen.load(Ordering::Acquire) {
            return Ok(());
        }
        self.inner
            .settings
            .persist_window_placement(bounds, maximized)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn freeze_persistence(&self) {
        let _persistence_guard = self
            .inner
            .persistence_gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.inner.persistence_frozen.store(true, Ordering::Release);
        self.inner
            .persistence_revision
            .fetch_add(1, Ordering::AcqRel);
    }
}

pub(crate) fn attach_placement_persistence(window: &WebviewWindow, state: NativeWindowState) {
    let tracked_window = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
            state.schedule_placement_persistence(tracked_window.clone());
        }
    });
}

pub(crate) fn monitor_work_areas(app: &tauri::App) -> tauri::Result<Vec<MonitorBounds>> {
    app.available_monitors()
        .map(|monitors| monitors.iter().map(monitor_work_area).collect())
}

pub(crate) fn primary_work_area(app: &tauri::App) -> tauri::Result<Option<MonitorBounds>> {
    app.primary_monitor()
        .map(|monitor| monitor.as_ref().map(monitor_work_area))
}

pub(crate) fn fit_inner_size_to_work_area(
    window: &WebviewWindow,
    work_area: MonitorBounds,
) -> tauri::Result<()> {
    let scale_factor = window.scale_factor()?;
    let outer = window.outer_size()?.to_logical::<f64>(scale_factor);
    let inner = window.inner_size()?.to_logical::<f64>(scale_factor);
    let decoration_width = (outer.width - inner.width).max(0.0);
    let decoration_height = (outer.height - inner.height).max(0.0);
    let width = (f64::from(work_area.width) - decoration_width).max(1.0);
    let height = (f64::from(work_area.height) - decoration_height).max(1.0);
    window.set_size(tauri::LogicalSize::new(width, height))
}

pub(crate) fn logical_inner_size(window: &WebviewWindow) -> Result<(u32, u32), String> {
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let size = window
        .inner_size()
        .map_err(|error| error.to_string())?
        .to_logical::<f64>(scale_factor);
    Ok((
        size.width.round().max(0.0) as u32,
        size.height.round().max(0.0) as u32,
    ))
}

fn monitor_work_area(monitor: &tauri::Monitor) -> MonitorBounds {
    let area = monitor.work_area();
    let scale_factor = monitor.scale_factor();
    let position = area.position.to_logical::<f64>(scale_factor);
    let size = area.size.to_logical::<f64>(scale_factor);
    MonitorBounds {
        x: position.x.round() as i32,
        y: position.y.round() as i32,
        width: size.width.round().max(0.0) as u32,
        height: size.height.round().max(0.0) as u32,
    }
}

fn window_bounds(window: &WebviewWindow) -> Result<WindowBounds, String> {
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let position = window
        .outer_position()
        .map_err(|error| error.to_string())?
        .to_logical::<f64>(scale_factor);
    let size = window
        .inner_size()
        .map_err(|error| error.to_string())?
        .to_logical::<f64>(scale_factor);
    Ok(WindowBounds {
        x: position.x.round() as i32,
        y: position.y.round() as i32,
        width: size.width.round().max(0.0) as u32,
        height: size.height.round().max(0.0) as u32,
    })
}

#[tauri::command]
pub(crate) async fn window_set_minimize_to_tray_on_close(
    state: tauri::State<'_, NativeWindowState>,
    enabled: bool,
) -> Result<(), DesktopCommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.set_minimize_to_tray_on_close(enabled))
        .await
        .map_err(|error| {
            DesktopCommandError::new(
                "settings_blocking_task_failed",
                "window_set_minimize_to_tray_on_close",
                format!("settings persistence task failed: {error}"),
                "MCode could not save the close-to-tray setting. Try again.",
                None,
                None,
            )
        })?
        .map_err(|error| {
            DesktopCommandError::settings(error, "window_set_minimize_to_tray_on_close")
        })
}

#[tauri::command]
pub(crate) fn window_hide_to_tray_if_enabled(
    window: WebviewWindow,
    state: tauri::State<'_, NativeWindowState>,
    lifecycle: tauri::State<'_, LifecycleService>,
) -> Result<bool, DesktopCommandError> {
    let shutdown_in_progress = lifecycle.is_shutdown_in_progress().map_err(|error| {
        DesktopCommandError::lifecycle(error, "window_hide_to_tray_if_enabled", None, None)
    })?;
    if !should_hide_close_to_tray(
        cfg!(target_os = "windows"),
        state.tray_available(),
        state.minimize_to_tray_on_close(),
        shutdown_in_progress,
    ) {
        return Ok(false);
    }
    window.hide().map_err(|error| {
        DesktopCommandError::new(
            "window_hide_failed",
            "window_hide_to_tray_if_enabled",
            error.to_string(),
            "MCode could not hide the window to the system tray. Try again.",
            None,
            None,
        )
    })?;
    Ok(true)
}
