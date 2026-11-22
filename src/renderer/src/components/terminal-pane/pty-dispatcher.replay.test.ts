// Why: replay is incremental scrollback over a dedicated push channel — the
// Tauri dispatcher port must fan each pty://replay payload out exactly like
// the Electron window.api.pty.onReplay branch (pty-spawn-result.ts: "must not
// clear the terminal before replay"), with shutdown-buffering consulted first.
//
// Tests below import the dispatcher modules dynamically on purpose: the
// singleton module state (event port, handler maps, watchdog) must reset per
// test via vi.resetModules(), the same pattern pty-dispatcher-push-reattach
// uses. Static imports would bind to a single stale module instance.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { installIpcPtyWindow, restorePtySpecWindow } from './pty-transport-test-harness'
import type { PtyApi } from '../../desktop/pty-client'

type ReplayCallback = (payload: { id: string; data: string }) => void

function buildReplayClientDouble(
  onReplay: (listener: (payload: { id: number; data: string }) => void) => Promise<() => void>
): PtyApi {
  const client: PtyApi = {
    spawn: vi.fn(),
    write: vi.fn(),
    resize: vi.fn(),
    kill: vi.fn(),
    hasPty: vi.fn(),
    ackData: vi.fn(),
    getSize: vi.fn(),
    onData: () => Promise.resolve(() => {}),
    onExit: () => Promise.resolve(() => {}),
    onWriteUnavailable: () => Promise.resolve(() => {}),
    onReplay
  }
  return client
}

describe('pty dispatcher replay delivery', () => {
  const originalWindow = (globalThis as { window?: typeof window }).window

  let portReplayCallbacks: ((payload: { id: number; data: string }) => void)[] = []

  let electronReplayCallbacks: ReplayCallback[] = []

  beforeEach(() => {
    vi.resetModules()
    portReplayCallbacks = []
    electronReplayCallbacks = []
  })

  afterEach(async () => {
    // Why: ensurePtyDispatcher started the watchdog's real-timer interval;
    // stop it so no tick outlives this file's mocked window.
    const { stopTerminalDeliveryWatchdog } = await import('./terminal-delivery-watchdog')
    stopTerminalDeliveryWatchdog()
    restorePtySpecWindow(originalWindow)
  })

  async function attachWithPort(): Promise<void> {
    const { installDispatcherEventPort } = await import('./pty-transport-tauri-dedupe')
    const { ensurePtyDispatcher } = await import('./pty-dispatcher')
    installDispatcherEventPort(
      buildReplayClientDouble((listener) => {
        portReplayCallbacks.push(listener)
        return Promise.resolve(() => {})
      })
    )
    ensurePtyDispatcher()
  }

  it('routes pty://replay through the Tauri port into ptyReplayHandlers with string ids', async () => {
    await attachWithPort()
    const { ptyReplayHandlers } = await import('./pty-dispatcher')

    const received: string[] = []
    ptyReplayHandlers.set('9', (data) => received.push(data))
    try {
      expect(portReplayCallbacks).toHaveLength(1)
      portReplayCallbacks[0]?.({ id: 9, data: 'scrollback-one' })
      portReplayCallbacks[0]?.({ id: 9, data: 'scrollback-two' })
      // Every payload forwards — no seq-dedupe on the replay channel.
      expect(received).toEqual(['scrollback-one', 'scrollback-two'])
    } finally {
      ptyReplayHandlers.delete('9')
    }
  })

  it('buffers replay behind a pending shutdown instead of delivering to the handler', async () => {
    await attachWithPort()
    const { ptyReplayHandlers, unregisterPtyDataHandlers } = await import('./pty-dispatcher')
    const received: string[] = []
    ptyReplayHandlers.set('9', (data) => received.push(data))
    const { ptyDataHandlers } = await import('./pty-dispatcher')
    ptyDataHandlers.set('9', () => {})
    const snapshots = unregisterPtyDataHandlers(['9'])
    try {
      expect(portReplayCallbacks).toHaveLength(1)
      portReplayCallbacks[0]?.({ id: 9, data: 'during-shutdown' })
      expect(received).toEqual([])

      // Failed shutdown rolls back: the buffered replay drains to the handler.
      snapshots[0]?.rollback()
      expect(received).toEqual(['during-shutdown'])
    } finally {
      ptyReplayHandlers.delete('9')
      ptyDataHandlers.delete('9')
    }
  })

  it('ignores replay for a pty id with no registered handler', async () => {
    await attachWithPort()
    const { ptyReplayHandlers } = await import('./pty-dispatcher')
    expect(ptyReplayHandlers.size).toBe(0)
    expect(() => portReplayCallbacks[0]?.({ id: 404, data: 'unrouted' })).not.toThrow()
  })

  it('keeps the Electron onReplay branch delivering straight to ptyReplayHandlers', async () => {
    installIpcPtyWindow(originalWindow, {
      replay: (callback) => {
        electronReplayCallbacks.push(callback)
      }
    })
    const { ensurePtyDispatcher, ptyReplayHandlers } = await import('./pty-dispatcher')
    ensurePtyDispatcher()

    const received: string[] = []
    ptyReplayHandlers.set('7', (data) => received.push(data))
    try {
      expect(electronReplayCallbacks).toHaveLength(1)
      electronReplayCallbacks[0]?.({ id: '7', data: 'electron-replay' })
      expect(received).toEqual(['electron-replay'])
    } finally {
      ptyReplayHandlers.delete('7')
    }
  })
})
