// @vitest-environment happy-dom
import assert from 'node:assert/strict'
import { expect, it as test, vi } from 'vitest'
import {
  createTauriUiBridgeClient,
  UI_APP_MENU_PASTE_EVENT,
  UI_APP_MENU_UNDO_EVENT,
  UI_APP_MENU_REDO_EVENT,
  UI_APP_MENU_SELECTION_EVENT,
  UI_TERMINAL_ZOOM_EVENT,
  UI_TOGGLE_LEFT_SIDEBAR_EVENT,
  UI_TOGGLE_RIGHT_SIDEBAR_EVENT,
  UI_TOGGLE_STATUS_BAR_EVENT,
  UI_TOGGLE_APPEARANCE_SETTING_EVENT,
  UI_CHECK_FOR_UPDATES_EVENT
} from './tauri-ui-bridge-client'

type RecordedListener = (event: { payload: unknown }) => void

function recordedTransport(handlers: Record<string, () => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler()
  }
  const listeners = new Map<string, RecordedListener[]>()
  const listen = async (event: string, listener: RecordedListener) => {
    const bucket = listeners.get(event) ?? []
    bucket.push(listener)
    listeners.set(event, bucket)
    return () => {
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((entry) => entry !== listener)
      )
    }
  }
  const emit = (event: string, payload: unknown): void => {
    for (const listener of listeners.get(event) ?? []) {
      listener({ payload })
    }
  }
  return { calls, invoke, listen, emit }
}

// The async listen() arming needs a microtask flush before emits land.
const flush = (): Promise<void> => Promise.resolve()

test('event names mirror the Rust app_menu.rs EVENT_* constants byte-for-byte', () => {
  assert.equal(UI_APP_MENU_PASTE_EVENT, 'ui:appMenuPaste')
  assert.equal(UI_APP_MENU_UNDO_EVENT, 'ui:appMenuUndo')
  assert.equal(UI_APP_MENU_REDO_EVENT, 'ui:appMenuRedo')
  assert.equal(UI_APP_MENU_SELECTION_EVENT, 'ui:appMenuSelectionAction')
  assert.equal(UI_TERMINAL_ZOOM_EVENT, 'terminal:zoom')
  assert.equal(UI_TOGGLE_LEFT_SIDEBAR_EVENT, 'ui:toggleLeftSidebar')
  assert.equal(UI_TOGGLE_RIGHT_SIDEBAR_EVENT, 'ui:toggleRightSidebar')
  assert.equal(UI_TOGGLE_STATUS_BAR_EVENT, 'ui:toggleStatusBar')
  assert.equal(UI_TOGGLE_APPEARANCE_SETTING_EVENT, 'ui:toggleAppearanceSetting')
  assert.equal(UI_CHECK_FOR_UPDATES_EVENT, 'ui:checkForUpdates')
})

test('void events deliver plain callbacks; unsubscribes remove the listener', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriUiBridgeClient(invoke, listen)
  await flush()

  const paste = vi.fn()
  const undo = vi.fn()
  const redo = vi.fn()
  const left = vi.fn()
  const right = vi.fn()
  const statusBar = vi.fn()
  const updates = vi.fn()
  const offPaste = client.onAppMenuPaste(paste)
  const offUndo = client.onAppMenuUndo(undo)
  const offRedo = client.onAppMenuRedo(redo)
  client.onToggleLeftSidebar(left)
  client.onToggleRightSidebar(right)
  client.onToggleStatusBar(statusBar)
  client.onCheckForUpdates(updates)
  await flush()

  emit(UI_APP_MENU_PASTE_EVENT, '')
  emit(UI_APP_MENU_UNDO_EVENT, '')
  emit(UI_APP_MENU_REDO_EVENT, '')
  emit(UI_TOGGLE_LEFT_SIDEBAR_EVENT, '')
  emit(UI_TOGGLE_RIGHT_SIDEBAR_EVENT, '')
  emit(UI_TOGGLE_STATUS_BAR_EVENT, '')
  emit(UI_CHECK_FOR_UPDATES_EVENT, '')
  expect(paste).toHaveBeenCalledTimes(1)
  expect(undo).toHaveBeenCalledTimes(1)
  expect(redo).toHaveBeenCalledTimes(1)
  expect(left).toHaveBeenCalledTimes(1)
  expect(right).toHaveBeenCalledTimes(1)
  expect(statusBar).toHaveBeenCalledTimes(1)
  expect(updates).toHaveBeenCalledTimes(1)

  offPaste()
  offUndo()
  offRedo()
  emit(UI_APP_MENU_PASTE_EVENT, '')
  emit(UI_APP_MENU_UNDO_EVENT, '')
  emit(UI_APP_MENU_REDO_EVENT, '')
  expect(paste).toHaveBeenCalledTimes(1)
  expect(undo).toHaveBeenCalledTimes(1)
  expect(redo).toHaveBeenCalledTimes(1)
})

test('payload events validate and forward their typed payload', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriUiBridgeClient(invoke, listen)
  await flush()

  const selection = vi.fn()
  const zoom = vi.fn()
  const appearance = vi.fn()
  client.onAppMenuSelectionAction(selection)
  client.onTerminalZoom(zoom)
  client.onToggleAppearanceSetting(appearance)
  await flush()

  emit(UI_APP_MENU_SELECTION_EVENT, 'copy')
  emit(UI_APP_MENU_SELECTION_EVENT, 'select-all')
  emit(UI_APP_MENU_SELECTION_EVENT, 'bogus')
  emit(UI_TERMINAL_ZOOM_EVENT, 'in')
  emit(UI_TERMINAL_ZOOM_EVENT, 'out')
  emit(UI_TERMINAL_ZOOM_EVENT, 'reset')
  emit(UI_TERMINAL_ZOOM_EVENT, 'sideways')
  emit(UI_TOGGLE_APPEARANCE_SETTING_EVENT, 'showTasksButton')
  emit(UI_TOGGLE_APPEARANCE_SETTING_EVENT, 'statusBarVisible')
  emit(UI_TOGGLE_APPEARANCE_SETTING_EVENT, 'not-a-key')

  expect(selection.mock.calls).toEqual([['copy'], ['select-all']])
  expect(zoom.mock.calls).toEqual([['in'], ['out'], ['reset']])
  expect(appearance.mock.calls).toEqual([['showTasksButton']])
})

test('popupMenu maps onto ui_popup_app_menu; unsubscribe before arming is safe', async () => {
  const { calls, invoke, listen } = recordedTransport({ ui_popup_app_menu: () => null })
  const client = createTauriUiBridgeClient(invoke, listen)

  const off = client.onAppMenuPaste(() => {})
  off()
  await flush()

  await client.popupMenu()
  assert.deepEqual(
    calls.map(({ command }) => command),
    ['ui_popup_app_menu']
  )
})

test('performNativeSelectionAction falls back to document.execCommand', () => {
  const { invoke, listen } = recordedTransport({})
  const client = createTauriUiBridgeClient(invoke, listen)
  const calls: string[] = []
  const original = (document as unknown as Record<string, unknown>).execCommand
  ;(document as unknown as Record<string, unknown>).execCommand = (command: string): boolean => {
    calls.push(command)
    return true
  }
  try {
    client.performNativeSelectionAction('copy')
    client.performNativeSelectionAction('select-all')
    assert.deepEqual(calls, ['copy', 'selectAll'])
  } finally {
    if (original === undefined) {
      delete (document as unknown as Record<string, unknown>).execCommand
    } else {
      ;(document as unknown as Record<string, unknown>).execCommand = original
    }
  }
})

test('passthrough listeners map unmapped on* methods onto their preload channels', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriUiBridgeClient(invoke, listen)
  await flush()

  const received: unknown[] = []
  const off = (
    client as unknown as Record<string, (cb: (payload: unknown) => void) => () => void>
  ).onOpenWorkspaceBoard((payload) => received.push(payload))
  await flush()
  emit('ui:openWorkspaceBoard', { board: true })
  off()
  emit('ui:openWorkspaceBoard', { late: true })
  await flush()

  assert.deepEqual(received, [{ board: true }])
})

test('unknown on* method access returns undefined (no crash)', () => {
  const { invoke, listen } = recordedTransport({})
  const client = createTauriUiBridgeClient(invoke, listen) as unknown as Record<string, unknown>
  assert.equal(client.onNotARealListenerAnywhere, undefined)
})
