import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createTauriPtyTransport } from './pty-transport-tauri'
import { applyPtySeqDedupe } from './pty-transport-tauri-dedupe'
import {
  ptyDataHandlers,
  ptyExitHandlers,
  reattachPtyDispatcherPushListeners,
  setPtyDispatcherEventPort
} from './pty-dispatcher'
import {
  clearProcessedPtyCharTotal,
  getProcessedPtyCharTotals,
  ackPtyData
} from './terminal-pty-ack-gate'
import { flushPtySideEffects } from './pty-transport-test-harness'
import type { PtyDataPayload, PtyExitPayload } from '../../desktop/pty-client'
import { createTauriPtyClient } from '../../desktop/pty-client'
// The dispatcher port routes through the ack gate, whose window.api.pty.ackData
// is absent in this environment — the Tauri no-op delta (§2.3).
vi.mock('@/lib/e2e-config', () => ({ e2eConfig: { exposeStore: false } }))

type InvokeCall = { command: string; args?: Record<string, unknown> }

function recordedInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: InvokeCall[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command ${command}`)
    }
    return handler(args ?? {})
  }
  return { calls, invoke }
}

/** Event-source double that hands the spec every subscription callback. */
function captureListen() {
  const dataSinks: ((payload: PtyDataPayload) => void)[] = []
  const exitSinks: ((payload: PtyExitPayload) => void)[] = []
  const listen = async (
    event: string,
    listener: (payload: { payload: unknown }) => void
  ): Promise<() => void> => {
    if (event === 'pty://data') {
      dataSinks.push((payload) => listener({ payload }))
    } else if (event === 'pty://exit') {
      exitSinks.push((payload) => listener({ payload }))
    }
    return () => {}
  }
  return {
    listen,
    emitData: (payload: PtyDataPayload) => dataSinks.forEach((sink) => sink(payload)),
    emitExit: (payload: PtyExitPayload) => exitSinks.forEach((sink) => sink(payload))
  }
}
describe('applyPtySeqDedupe (§2.3 pure helper)', () => {
  it('skips chunks whose covered span starts before the baseline and keeps equal', () => {
    // Chunk spans are [startSeq, seq): 'def' covers 3..6 and 'ghi' covers 6..9.
    // Baseline 6: 'def' starts below it (skip), 'ghi' starts exactly at it (keep).
    const { chunks } = applyPtySeqDedupe(
      {
        chunks: [
          { data: 'abc', seq: 3 },
          { data: 'def', seq: 6 },
          { data: 'ghi', seq: 9 }
        ]
      },
      6
    )
    expect(chunks).toEqual([{ data: 'ghi', seq: 9 }])
  })

  it('computes the overlap from the source span when rawLength is present', () => {
    // The chunk covers seq 4..10 (rawLength 6, delivered length 3):
    // startSeq = 10 - 6 = 4, below the baseline 5 → skipped.
    const { chunks } = applyPtySeqDedupe(
      { chunks: [{ data: 'abc', seq: 10, rawLength: 6, transformed: true }] },
      5
    )
    expect(chunks).toEqual([])
  })

  it('resets the baseline to markerSeq when a restoreMarker is present', () => {
    const { chunks, nextBaseline } = applyPtySeqDedupe(
      {
        chunks: [
          { data: 'a', seq: 2 },
          { data: 'b', seq: 12 }
        ],
        restoreMarker: { reason: 'hidden_drop', markerSeq: 10 }
      },
      11
    )
    expect(chunks).toEqual([{ data: 'b', seq: 12 }])
    expect(nextBaseline).toBe(10)
  })
})

describe('createTauriPtyTransport', () => {
  const originalWindow = (globalThis as { window?: typeof window }).window

  beforeEach(() => {
    clearProcessedPtyCharTotal('7')
    clearProcessedPtyCharTotal('9')
    clearProcessedPtyCharTotal('11')
    setPtyDispatcherEventPort(null)
  })

  afterEach(() => {
    restorePtySpecWindow(originalWindow)
    setPtyDispatcherEventPort(null)
  })

  function restorePtySpecWindow(original: typeof window | undefined): void {
    if (original) {
      ;(globalThis as { window: typeof window }).window = original
    } else {
      delete (globalThis as { window?: typeof window }).window
    }
  }

  function buildTransport(
    handlers: Record<string, (args: Record<string, unknown>) => unknown>,
    opts: Parameters<typeof createTauriPtyTransport>[0] = {
      cwd: 'D:/repo',
      shellOverride: 'pwsh.exe'
    }
  ) {
    const { calls, invoke } = recordedInvoke(handlers)
    const events = captureListen()
    const transport = createTauriPtyTransport(opts, {
      invoke,
      listen: events.listen,
      client: createTauriPtyClient(invoke, events.listen)
    })
    return { calls, transport, events }
  }

  it('maps connect/sendInput/resize/disconnect onto pty_spawn/pty_write/pty_resize/pty_kill with Number ids', async () => {
    const { calls, transport } = buildTransport({
      pty_spawn: () => ({ id: 7, shell: 'pwsh.exe' }),
      pty_write: () => null,
      pty_resize: () => null,
      pty_kill: () => null
    })

    const result = await transport.connect({ url: '', cols: 100, rows: 30, callbacks: {} })
    expect(result).toBe('7')
    expect(transport.getPtyId()).toBe('7')
    expect(transport.isConnected?.()).toBe(true)
    expect(transport.getConnectionId?.()).toBeNull()
    expect(transport.getLocalSessionMetadata?.()).toEqual({
      cwd: 'D:/repo',
      shellOverride: 'pwsh.exe'
    })

    expect(calls).toHaveLength(1)
    expect(calls[0].command).toBe('pty_spawn')
    expect(calls[0].args).toEqual({
      spec: {
        cols: 100,
        rows: 30,
        cwd: 'D:/repo',
        shellOverride: 'pwsh.exe'
      }
    })

    expect(transport.sendInput('dir\r\n')).toBe(true)
    await vi.waitFor(() => {
      expect(calls.some((call) => call.command === 'pty_write')).toBe(true)
    })
    expect(calls.find((call) => call.command === 'pty_write')?.args).toEqual({
      id: 7,
      data: 'dir\r\n'
    })

    expect(transport.resize(120, 40)).toBe(true)
    expect(calls.find((call) => call.command === 'pty_resize')?.args).toEqual({
      id: 7,
      cols: 120,
      rows: 40
    })

    transport.disconnect()
    expect(calls.find((call) => call.command === 'pty_kill')?.args).toEqual({ id: 7 })
    expect(transport.isConnected()).toBe(false)
    expect(transport.getPtyId()).toBeNull()
  })

  it('fans a two-chunk PtyDataPayload out into two router events with string ids', async () => {
    const { transport, events } = buildTransport({
      pty_spawn: () => ({ id: 9, shell: 'pwsh.exe' })
    })
    await transport.connect({ url: '', callbacks: {} })

    const dataEvents: { id: string; data: string; seq?: number; rawLength?: number }[] = []
    ptyDataHandlers.set('9', (data, meta) => {
      dataEvents.push({ id: '9', data, seq: meta?.seq, rawLength: meta?.rawLength })
    })
    try {
      // Settle the async dispatcher re-subscription through the port this
      // transport just installed (the singleton attach happened on an earlier
      // port, or a previous test consumed it).
      reattachPtyDispatcherPushListeners()
      await flushPtySideEffects()
      // The Rust emit path: one PtyDataPayload{id: number, chunks: []} — the
      // installed port adapter fans it out per chunk with String(id).
      events.emitData({
        id: 9,
        chunks: [
          { data: 'hello ', seq: 6 },
          { data: 'wor', seq: 15, rawLength: 6, transformed: true }
        ]
      })
      expect(dataEvents).toEqual([
        { id: '9', data: 'hello ', seq: 6, rawLength: undefined },
        { id: '9', data: 'wor', seq: 15, rawLength: 6 }
      ])
    } finally {
      ptyDataHandlers.delete('9')
    }
  })

  it('skips a chunk whose startSeq is below the processed baseline and keeps an equal one', async () => {
    const { transport, events } = buildTransport({
      pty_spawn: () => ({ id: 9, shell: 'pwsh.exe' })
    })
    await transport.connect({ url: '', callbacks: {} })

    const dataEvents: string[] = []
    ptyDataHandlers.set('9', (data) => {
      dataEvents.push(data)
    })
    try {
      // Settle the async dispatcher re-subscription through the port this
      // transport just installed (the singleton attach happened on an earlier
      // port, or a previous test consumed it).
      reattachPtyDispatcherPushListeners()
      await flushPtySideEffects()
      // Baseline advances: ack 6 processed chars for pty '9'.
      ackPtyData('9', 6)
      expect(getProcessedPtyCharTotals()['9']).toBe(6)

      events.emitData({
        id: 9,
        chunks: [
          { data: 'old', seq: 4 }, // startSeq 1 < 6 → skipped
          { data: 'new', seq: 9 } // startSeq 6 === baseline → kept
        ]
      })
      expect(dataEvents).toEqual(['new'])
    } finally {
      ptyDataHandlers.delete('9')
    }
  })

  it('restoreMarker resets the dedupe baseline for subsequent payloads', async () => {
    const { transport, events } = buildTransport({
      pty_spawn: () => ({ id: 9, shell: 'pwsh.exe' })
    })
    await transport.connect({ url: '', callbacks: {} })

    const dataEvents: string[] = []
    ptyDataHandlers.set('9', (data) => {
      dataEvents.push(data)
    })
    try {
      // Settle the async dispatcher re-subscription through the port this
      // transport just installed (the singleton attach happened on an earlier
      // port, or a previous test consumed it).
      reattachPtyDispatcherPushListeners()
      await flushPtySideEffects()
      ackPtyData('9', 20)
      // Without the marker, startSeq 13 (seq 14, 1-char chunk) < baseline 20
      // would be skipped…
      events.emitData({
        id: 9,
        chunks: [{ data: 'x', seq: 14 }],
        restoreMarker: { reason: 'hidden_drop', markerSeq: 10 }
      })
      // …but markerSeq 10 resets the window, so startSeq 13 is delivered.
      expect(dataEvents).toEqual(['x'])

      // Subsequent payloads dedupe against the markerSeq baseline: seq 10
      // covers 3..10 (below), seq 11 covers 10..11 (exactly at the window).
      dataEvents.length = 0
      events.emitData({ id: 9, chunks: [{ data: 'covered', seq: 10 }] })
      expect(dataEvents).toEqual([])
      events.emitData({ id: 9, chunks: [{ data: 'f', seq: 11 }] })
      expect(dataEvents).toEqual(['f'])
    } finally {
      ptyDataHandlers.delete('9')
    }
  })

  it('routes PtyExitPayload through the dispatcher once and clears per-id state', async () => {
    const onExitSpy = vi.fn()
    const onDisconnectSpy = vi.fn()
    const onPtyExitSpy = vi.fn()
    const { transport, events } = buildTransport(
      { pty_spawn: () => ({ id: 11, shell: 'pwsh.exe' }) },
      { onPtyExit: onPtyExitSpy }
    )
    await transport.connect({
      url: '',
      callbacks: { onExit: onExitSpy, onDisconnect: onDisconnectSpy }
    })

    // The transport registered its own exit handler at connect (which forwards
    // to storedCallbacks/onPtyExit); assert through it instead of overwriting.
    expect(ptyExitHandlers.has('11')).toBe(true)
    // Settle the async dispatcher re-subscription through the port this
    // transport just installed (the singleton attach happened on an earlier
    // port, or a previous test consumed it).
    reattachPtyDispatcherPushListeners()
    await flushPtySideEffects()
    events.emitExit({ id: 11, code: 3 })
    expect(onExitSpy).toHaveBeenCalledWith(3)
    expect(onDisconnectSpy).toHaveBeenCalledOnce()
    expect(onPtyExitSpy).toHaveBeenCalledWith('11')
    expect(transport.isConnected()).toBe(false)
    expect(transport.getPtyId()).toBeNull()
    // One-shot owner: the dispatcher removed the handler before invoking.
    expect(ptyExitHandlers.has('11')).toBe(false)
    // Exit also clears the dedupe accounting for the id.
    expect(getProcessedPtyCharTotals()['11']).toBeUndefined()
  })
})
