import { createDesktopInvoke } from './desktop-client'

// ─── Types (verbatim mirror of the menu-event surface of
// src/preload/api/ui-command-event-api.ts + ui-window-api.ts) ────────────────

export type AppMenuSelectionAction = 'copy' | 'select-all'
export type TerminalZoomDirection = 'in' | 'out' | 'reset'
/** ui-window-api.ts UIZoomControl source of truth (register-app-menu.ts appearance keys). */
export type AppearanceSettingKey =
  | 'showTasksButton'
  | 'showAutomationsButton'
  | 'showMobileButton'
  | 'showTitlebarAppName'

export type UiSubscription = () => void

/**
 * The menu-event subset of `window.api.ui` served natively under Tauri.
 * Event names and payloads mirror the original main-process sends exactly
 * (app_menu.rs EVENT_* constants, asserted byte-for-byte in tests).
 */
export type TauriUiBridgeApi = {
  popupMenu: () => Promise<void>
  performNativeSelectionAction: (action: AppMenuSelectionAction) => void
  onAppMenuPaste: (callback: () => void) => UiSubscription
  onAppMenuUndo: (callback: () => void) => UiSubscription
  onAppMenuRedo: (callback: () => void) => UiSubscription
  onAppMenuSelectionAction: (callback: (action: AppMenuSelectionAction) => void) => UiSubscription
  onTerminalZoom: (callback: (direction: TerminalZoomDirection) => void) => UiSubscription
  onToggleLeftSidebar: (callback: () => void) => UiSubscription
  onToggleRightSidebar: (callback: () => void) => UiSubscription
  onToggleStatusBar: (callback: () => void) => UiSubscription
  onToggleAppearanceSetting: (callback: (key: AppearanceSettingKey) => void) => UiSubscription
  onCheckForUpdates: (callback: () => void) => UiSubscription
}

type UiInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

type UiEventSource = (
  event: string,
  listener: (event: { payload: unknown }) => void
) => Promise<() => void>

// app_menu.rs EVENT_* constants — the wire contract with the Rust dispatcher.
export const UI_APP_MENU_PASTE_EVENT = 'ui:appMenuPaste'
export const UI_APP_MENU_UNDO_EVENT = 'ui:appMenuUndo'
export const UI_APP_MENU_REDO_EVENT = 'ui:appMenuRedo'
export const UI_APP_MENU_SELECTION_EVENT = 'ui:appMenuSelectionAction'
export const UI_TERMINAL_ZOOM_EVENT = 'terminal:zoom'
export const UI_TOGGLE_LEFT_SIDEBAR_EVENT = 'ui:toggleLeftSidebar'
export const UI_TOGGLE_RIGHT_SIDEBAR_EVENT = 'ui:toggleRightSidebar'
export const UI_TOGGLE_STATUS_BAR_EVENT = 'ui:toggleStatusBar'
export const UI_TOGGLE_APPEARANCE_SETTING_EVENT = 'ui:toggleAppearanceSetting'
export const UI_CHECK_FOR_UPDATES_EVENT = 'ui:checkForUpdates'

const APPEARANCE_KEYS: readonly AppearanceSettingKey[] = [
  'showTasksButton',
  'showAutomationsButton',
  'showMobileButton',
  'showTitlebarAppName'
]

function isAppearanceKey(value: unknown): AppearanceSettingKey | undefined {
  if (typeof value !== 'string') {
    return undefined
  }
  return (APPEARANCE_KEYS as readonly string[]).includes(value)
    ? (value as AppearanceSettingKey)
    : undefined
}

/**
 * Builds the typed UI bridge over any invoke/event transport. Raw command
 * strings live only here; listeners are synchronous-unsubscribe to match the
 * preload contract (`ui-command-event-api.ts`: every `on*` returns `() => void`).
 * Tauri `listen()` is async, so subscription arming is deferred behind a
 * microtask — the returned unsubscribe is safe to call before arming.
 */
export function createTauriUiBridgeClient(
  invoke: UiInvoke,
  listen: UiEventSource
): TauriUiBridgeApi {
  const call = createDesktopInvoke(invoke)

  function subscribe<T>(
    event: string,
    guard: (payload: unknown) => T | undefined,
    callback: (value: T) => void
  ): UiSubscription {
    let unlisten: (() => void) | undefined
    let disposed = false
    void listen(event, (e) => {
      const value = guard(e.payload)
      if (value !== undefined) {
        callback(value)
      }
    }).then((off) => {
      if (disposed) {
        off()
      } else {
        unlisten = off
      }
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }

  const voidEvent =
    (event: string) =>
    (callback: () => void): UiSubscription =>
      subscribe(event, () => true, callback)

  return {
    popupMenu: async () => {
      await call('ui_popup_app_menu')
    },
    performNativeSelectionAction: (action) => {
      // ui-window-api.ts contract: main-process native selection when no
      // renderer surface claims the action (useAppMenuSelectionActions.ts:11
      // fallback). Under wry there is no NSUserAction; execCommand is the
      // WebView-native equivalent for copy/select-all.
      document.execCommand(action === 'copy' ? 'copy' : 'selectAll')
    },
    onAppMenuPaste: voidEvent(UI_APP_MENU_PASTE_EVENT),
    onAppMenuUndo: voidEvent(UI_APP_MENU_UNDO_EVENT),
    onAppMenuRedo: voidEvent(UI_APP_MENU_REDO_EVENT),
    onAppMenuSelectionAction: (callback) =>
      subscribe(
        UI_APP_MENU_SELECTION_EVENT,
        (payload) => (payload === 'copy' || payload === 'select-all' ? payload : undefined),
        callback
      ),
    onTerminalZoom: (callback) =>
      subscribe(
        UI_TERMINAL_ZOOM_EVENT,
        (payload) =>
          payload === 'in' || payload === 'out' || payload === 'reset' ? payload : undefined,
        callback
      ),
    onToggleLeftSidebar: voidEvent(UI_TOGGLE_LEFT_SIDEBAR_EVENT),
    onToggleRightSidebar: voidEvent(UI_TOGGLE_RIGHT_SIDEBAR_EVENT),
    onToggleStatusBar: voidEvent(UI_TOGGLE_STATUS_BAR_EVENT),
    onToggleAppearanceSetting: (callback) =>
      subscribe(UI_TOGGLE_APPEARANCE_SETTING_EVENT, isAppearanceKey, callback),
    onCheckForUpdates: voidEvent(UI_CHECK_FOR_UPDATES_EVENT)
  }
}
