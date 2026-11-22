import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PtyDataPayload } from '../../desktop/pty-client'

vi.mock('@/lib/e2e-config', () => ({ e2eConfig: { exposeStore: false } }))
vi.mock('./terminal-freeze-report', () => ({ installTerminalFreezeReport: vi.fn() }))
vi.mock('./terminal-delivery-watchdog', () => ({
  startTerminalDeliveryWatchdog: vi.fn(), isPtyPushDeliveryBlackholed: () => false,
  recordPtyDataReceived: vi.fn(), clearReceivedPtyCharTotal: vi.fn()
}))

beforeEach(() => vi.resetModules())
afterEach(() => vi.unstubAllGlobals())

describe('Tauri PTY subscription and credit boundary', () => {
  it('does not spawn until every native listener is armed, retaining immediate output and exit', async () => {
    const { createTauriPtyTransport } = await import('./pty-transport-tauri')
    let releaseData = () => {}
    const dataReady = new Promise<void>((resolve) => { releaseData = resolve })
    const listeners = new Map<string, (event: { payload: unknown }) => void>()
    const onData = vi.fn()
    const onExit = vi.fn()
    const invoke = vi.fn(async (command: string) => {
      expect(command).toBe('pty_spawn')
      expect([...listeners.keys()]).toEqual([
        'pty://data', 'pty://exit', 'pty://replay', 'pty://write-unavailable'
      ])
      listeners.get('pty://data')?.({ payload: { id: 701, chunks: [{ data: '😀', seq: 2 }] } })
      listeners.get('pty://exit')?.({ payload: { id: 701, code: 0 } })
      return { id: 701, shell: 'cmd' }
    })
    const transport = createTauriPtyTransport({}, {
      invoke,
      listen: async (event, listener) => {
        if (event === 'pty://data') await dataReady
        listeners.set(event, listener)
        return () => { listeners.delete(event) }
      }
    })
    const connecting = transport.connect({ url: '', callbacks: { onData, onExit } })
    await Promise.resolve()
    await Promise.resolve()
    expect(invoke).not.toHaveBeenCalled()
    releaseData()
    await connecting
    expect(invoke).toHaveBeenCalledOnce()
    expect(onData).toHaveBeenCalledWith('😀', expect.anything())
    expect(onExit).toHaveBeenCalledWith(0)
  })

  it('cleans partial subscription failures and retries initialization before spawning', async () => {
    const { createTauriPtyTransport } = await import('./pty-transport-tauri')
    const removed = vi.fn()
    let fail = true
    const invoke = vi.fn(async () => ({ id: 702, shell: 'cmd' }))
    const onError = vi.fn()
    const transport = createTauriPtyTransport({}, {
      invoke,
      listen: async (event) => {
        if (event === 'pty://exit' && fail) throw new Error('listener unavailable')
        return removed
      }
    })
    await transport.connect({ url: '', callbacks: { onError } })
    expect(invoke).not.toHaveBeenCalled()
    expect(removed).toHaveBeenCalledOnce()
    expect(onError).toHaveBeenCalledWith('listener unavailable')
    fail = false
    await expect(transport.connect({ url: '', callbacks: {} })).resolves.toBe('702')
    expect(invoke).toHaveBeenCalledOnce()
  })

  it('forwards unsequenced dropped probes through the real dispatcher and cumulative UTF-16 ACK gate', async () => {
    const ackData = vi.fn()
    vi.stubGlobal('window', { api: { pty: { ackData } } })
    const { createTauriPtyClient } = await import('../../desktop/pty-client')
    const { installDispatcherEventPort } = await import('./pty-transport-tauri-dedupe')
    const { ensurePtyDispatcherReady, ptyDataHandlers } = await import('./pty-dispatcher')
    let emit: (payload: PtyDataPayload) => void = () => {}
    const client = createTauriPtyClient(async () => null, async (event, listener) => {
      if (event === 'pty://data') emit = (payload) => listener({ payload })
      return () => {}
    })
    installDispatcherEventPort(client)
    await ensurePtyDispatcherReady()
    const handler = vi.fn()
    ptyDataHandlers.set('703', handler)
    emit({ id: 703, chunks: [{ data: '😀', seq: 2 }] })
    emit({ id: 703, chunks: [], dropped: { data: '\x1b[6n', droppedOutput: true },
      restoreMarker: { reason: 'pending_cap', markerSeq: 10000 } })
    expect(handler).toHaveBeenLastCalledWith('\x1b[6n', { droppedOutput: true })
    expect(ackData).toHaveBeenNthCalledWith(1, '703', 2, 2)
    expect(ackData).toHaveBeenNthCalledWith(2, '703', 4, 6)
  })
})
