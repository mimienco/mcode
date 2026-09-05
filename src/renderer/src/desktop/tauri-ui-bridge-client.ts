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

// ─── Remaining preload channel map (src/preload/index.ts, extracted
// verbatim; docs/migration/ui-event-channel-map.json is the auditable
// source). Events whose Rust emitters have not landed yet simply never fire
// under Tauri — subscribing is harmless and keeps the renderer mounting
// instead of crashing on an undefined method. ──────────────────────────────
const PASSTHROUGH_LISTENER_CHANNELS: Readonly<Record<string, string>> = {
  onFocusBrowserAddressBar: 'ui:focusBrowserAddressBar',
  onFindInBrowserPage: 'ui:findInBrowserPage',
  onSystemResumed: 'system:resumed',
  onBrowserHistoryNavigate: 'ui:browserHistoryNavigate',
  onReloadBrowserPage: 'ui:reloadBrowserPage',
  onZoomBrowserPage: 'ui:zoomBrowserPage',
  onHardReloadBrowserPage: 'ui:hardReloadBrowserPage',
  onOpenCrashReport: 'ui:openCrashReport',
  onDictationKeyDown: 'ui:dictationKeyDown',
  onRichMarkdownContextCommand: 'rich-markdown:context-command',
  onFileDrop: 'terminal:file-drop',
  onCtrlTabKeyDown: 'ui:ctrlTabKeyDown',
  onCtrlTabKeyUp: 'ui:ctrlTabKeyUp',
  onMaximizeChanged: 'window:maximize-changed',
  onWindowCloseRequested: 'window:close-requested',
  onEditableContextPaste: 'ui:editableContextPaste',
  onOpenSettings: 'ui:openSettings',
  onOpenSkillShare: 'ui:openSkillShare',
  onOpenSetupGuide: 'ui:openSetupGuide',
  onOpenFeatureTour: 'ui:openFeatureTour',
  onStateChanged: 'ui:stateChanged',
  onToggleWorktreePalette: 'ui:toggleWorktreePalette',
  onToggleFloatingTerminal: 'ui:toggleFloatingTerminal',
  onTerminalShortcutCaptured: 'ui:terminalShortcutCaptured',
  onOpenQuickOpen: 'ui:openQuickOpen',
  onToggleQuickCommandsMenu: 'ui:toggleQuickCommandsMenu',
  onOpenNewWorkspace: 'ui:openNewWorkspace',
  onDeleteCurrentWorkspace: 'ui:deleteCurrentWorkspace',
  onOpenWorkspaceBoard: 'ui:openWorkspaceBoard',
  onOpenTasks: 'ui:openTasks',
  onJumpToWorktreeIndex: 'ui:jumpToWorktreeIndex',
  onJumpToTabIndex: 'ui:jumpToTabIndex',
  onWorktreeHistoryNavigate: 'ui:worktreeHistoryNavigate',
  onActivateWorktree: 'ui:activateWorktree',
  onCreateTerminal: 'ui:createTerminal',
  onRequestTerminalTabMount: 'ui:requestTerminalTabMount',
  onRequestTerminalCreate: 'ui:requestTerminalCreate',
  onSplitTerminal: 'ui:splitTerminal',
  onRenameTerminal: 'ui:renameTerminal',
  onFocusTerminal: 'ui:focusTerminal',
  onFocusEditorTab: 'ui:focusEditorTab',
  onCloseSessionTab: 'ui:closeSessionTab',
  onSessionTabCloseRequest: 'ui:sessionTabCloseRequest',
  onMoveSessionTab: 'ui:moveSessionTab',
  onOpenFileFromMobile: 'ui:openFileFromMobile',
  onOpenDiffFromMobile: 'ui:openDiffFromMobile',
  onCloseTerminal: 'ui:closeTerminal',
  onTerminalTabCloseRequest: 'ui:terminalTabCloseRequest',
  onSleepWorktree: 'ui:sleepWorktree',
  onResumeSleepingAgents: 'ui:resumeSleepingAgents',
  onFullscreenChanged: 'window:fullscreen-changed',
  onNewBrowserTab: 'ui:newBrowserTab',
  onNewMarkdownTab: 'ui:newMarkdownTab',
  onNewSimulatorTab: 'ui:newSimulatorTab',
  onRequestTabCreate: 'browser:requestTabCreate',
  onRequestTabSetProfile: 'ui:requestTabSetProfile',
  onRequestTabClose: 'ui:requestTabClose',
  onNewTerminalTab: 'ui:newTerminalTab',
  onCloseActiveTab: 'ui:closeActiveTab',
  onCloseFloatingItem: 'ui:closeFloatingItem',
  onSelectFloatingIndex: 'ui:selectFloatingIndex',
  onSwitchTab: 'ui:switchTab',
  onSwitchTabAcrossAllTypes: 'ui:switchTabAcrossAllTypes',
  onSwitchRecentTab: 'ui:switchRecentTab',
  onSwitchTerminalTab: 'ui:switchTerminalTab',
  onMobileMarkdownRequest: 'ui:mobileMarkdownRequest',
  onTerminalSerializeBufferRequest: 'terminal:serializeBufferRequest',
  onClearBufferRequest: 'terminal:clearBufferRequest',
  onWriteUnavailable: 'terminal:writeUnavailable',
  onDeliveryResyncRequest: 'terminal:deliveryResyncRequest',
  onSerializeBufferRequest: 'terminal:serializeBufferRequest'
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

  // Any on* method the renderer calls that the typed surface above does not
  // name is proxied onto its original Electron channel (preload/index.ts
  // channel map; docs/migration/ui-event-channel-map.json). Nothing emits
  // those channels under Tauri until the matching backend wave lands, so
  // the listener simply never fires — the renderer mounts instead of
  // crashing on an undefined method.
  const passthroughListeners = new Proxy({} as Record<string, unknown>, {
    get: (_target, method: string) => {
      const channel = PASSTHROUGH_LISTENER_CHANNELS[method]
      if (!channel) {
        return undefined
      }
      return (callback: (payload: unknown) => void): UiSubscription =>
        subscribe(channel, (payload) => payload, callback)
    }
  })

  return new Proxy(
    {
      popupMenu: async () => {
        await call('ui_popup_app_menu')
      },
      performNativeSelectionAction: (action: AppMenuSelectionAction) => {
        // ui-window-api.ts contract: main-process native selection when no
        // renderer surface claims the action (useAppMenuSelectionActions.ts:11
        // fallback). Under wry there is no NSUserAction; execCommand is the
        // WebView-native equivalent for copy/select-all.
        document.execCommand(action === 'copy' ? 'copy' : 'selectAll')
      },
      onAppMenuPaste: voidEvent(UI_APP_MENU_PASTE_EVENT),
      onAppMenuUndo: voidEvent(UI_APP_MENU_UNDO_EVENT),
      onAppMenuRedo: voidEvent(UI_APP_MENU_REDO_EVENT),
      onAppMenuSelectionAction: (callback: (action: AppMenuSelectionAction) => void) =>
        subscribe(
          UI_APP_MENU_SELECTION_EVENT,
          (payload) => (payload === 'copy' || payload === 'select-all' ? payload : undefined),
          callback
        ),
      onTerminalZoom: (callback: (direction: TerminalZoomDirection) => void) =>
        subscribe(
          UI_TERMINAL_ZOOM_EVENT,
          (payload) =>
            payload === 'in' || payload === 'out' || payload === 'reset' ? payload : undefined,
          callback
        ),
      onToggleLeftSidebar: voidEvent(UI_TOGGLE_LEFT_SIDEBAR_EVENT),
      onToggleRightSidebar: voidEvent(UI_TOGGLE_RIGHT_SIDEBAR_EVENT),
      onToggleStatusBar: voidEvent(UI_TOGGLE_STATUS_BAR_EVENT),
      onToggleAppearanceSetting: (callback: (key: AppearanceSettingKey) => void) =>
        subscribe(UI_TOGGLE_APPEARANCE_SETTING_EVENT, isAppearanceKey, callback),
      onCheckForUpdates: voidEvent(UI_CHECK_FOR_UPDATES_EVENT)
    } as unknown as Record<string, unknown>,
    {
      get: (target, method: string) => target[method] ?? passthroughListeners[method]
    }
  ) as TauriUiBridgeApi
}
