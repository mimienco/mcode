import { describe, expect, it, vi } from 'vitest'
import {
  createTauriTerminalPreviewClient,
  type TerminalPreviewInvoke,
  type TerminalPreviewListen
} from './tauri-terminal-preview-client'

type InvokeProbe = TerminalPreviewInvoke & {
  calls: { command: string; args?: Record<string, unknown> }[]
}

function createInvoke(resolve?: (command: string, args?: Record<string, unknown>) => unknown) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = (async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    return resolve?.(command, args)
  }) as InvokeProbe
  invoke.calls = calls
  return invoke
}

type ListenProbe = TerminalPreviewListen & {
  handler: ((event: { payload: unknown }) => void) | null
  unlisten: ReturnType<typeof vi.fn> | null
  emits: (payload: unknown) => void
}

function createListen() {
  const probe = {
    handler: null,
    unlisten: null
  } as ListenProbe
  const listen = (async (_event: string, handler: (event: { payload: unknown }) => void) => {
    probe.handler = handler
    probe.unlisten = vi.fn()
    return probe.unlisten
  }) as unknown as ListenProbe
  probe.emits = (payload: unknown) => probe.handler?.({ payload })
  return { listen, probe }
}

/** Live chunk tuple the Rust forwarder emits: (ptyId, TerminalPreviewChunk). */
function liveChunkTuple(seq: number, chunk: string): unknown {
  return ['pty-1', { seq, chunk, snapshot: null }]
}

describe('tauri-terminal-preview-client', () => {
  it('maps the connect payload into the Electron connect result shape', async () => {
    const invoke = createInvoke(() => ({ seq: 7, chunk: null, snapshot: 'screen-bytes' }))
    const client = createTauriTerminalPreviewClient(invoke, createListen().listen)

    await expect(client.connect('pty-1', { scrollbackRows: 24 })).resolves.toEqual({
      snapshot: { data: 'screen-bytes', cols: 80, rows: 24, seq: 7 },
      replay: []
    })
    expect(invoke.calls).toEqual([{ command: 'terminalPreviewConnect', args: { ptyId: 'pty-1' } }])
  })

  it('resolves the pty-gone result for a null or empty connect snapshot', async () => {
    const nullClient = createTauriTerminalPreviewClient(
      createInvoke(() => ({ seq: 1, chunk: null, snapshot: null })),
      createListen().listen
    )
    const emptyClient = createTauriTerminalPreviewClient(
      createInvoke(() => ({ seq: 1, chunk: null, snapshot: '' })),
      createListen().listen
    )

    await expect(nullClient.connect('pty-1')).resolves.toEqual({ snapshot: null, replay: [] })
    await expect(emptyClient.connect('pty-1')).resolves.toEqual({ snapshot: null, replay: [] })
  })

  it('resolves the pty-gone result when connect rejects instead of throwing', async () => {
    const failing = (async () => {
      throw 'terminal preview session gone'
    }) as unknown as TerminalPreviewInvoke
    const client = createTauriTerminalPreviewClient(failing, createListen().listen)

    await expect(client.connect('pty-1')).resolves.toEqual({ snapshot: null, replay: [] })
  })

  it('routes input, ack, fit, and unsubscribe to the renamed Rust commands', async () => {
    const invoke = createInvoke()
    const client = createTauriTerminalPreviewClient(invoke, createListen().listen)

    await expect(client.input('pty-1', 'ls\n')).resolves.toBe(true)
    await expect(client.ack('pty-1', 42)).resolves.toBeUndefined()
    await expect(client.fit('pty-1', 120, 40)).resolves.toEqual({ cols: 120, rows: 40 })
    await expect(client.unsubscribe('pty-1')).resolves.toBeUndefined()

    expect(invoke.calls).toEqual([
      { command: 'terminalPreviewInput', args: { ptyId: 'pty-1', data: 'ls\n' } },
      { command: 'terminalPreviewAck', args: { ptyId: 'pty-1', seq: 42 } },
      { command: 'terminalPreviewFit', args: { ptyId: 'pty-1', cols: 120, rows: 40 } },
      { command: 'terminalPreviewUnsubscribe', args: { ptyId: 'pty-1' } }
    ])
  })

  it('reports input failure as false and fit failure as null without throwing', async () => {
    const failing = (async (command: string) => {
      throw command === 'terminalPreviewInput'
        ? 'terminal preview session gone'
        : 'invalid pty id: pty-1'
    }) as unknown as TerminalPreviewInvoke
    const client = createTauriTerminalPreviewClient(failing, createListen().listen)

    await expect(client.input('pty-1', 'x')).resolves.toBe(false)
    await expect(client.fit('pty-1', 80, 24)).resolves.toBeNull()
  })

  it('maps live Rust data events into Electron payloads carrying the seq cursor', async () => {
    const { listen, probe } = createListen()
    const callback = vi.fn()
    const client = createTauriTerminalPreviewClient(createInvoke(), listen)

    const unlisten = client.onData(callback)
    expect(typeof unlisten).toBe('function')
    probe.emits(liveChunkTuple(9, 'live-bytes'))

    expect(callback).toHaveBeenCalledWith({
      type: 'data',
      ptyId: 'pty-1',
      data: 'live-bytes',
      bytes: 9
    })
  })

  it('drops non-live Rust events (connect echoes, snapshot replays) and malformed payloads', async () => {
    const { listen, probe } = createListen()
    const callback = vi.fn()
    const client = createTauriTerminalPreviewClient(createInvoke(), listen)
    client.onData(callback)

    probe.emits(['pty-1', { seq: 5, chunk: null, snapshot: 'snap' }])
    probe.emits(['pty-1', { noSeq: true }])
    probe.emits('garbage')
    probe.emits(['pty-1'])

    expect(callback).not.toHaveBeenCalled()
  })

  it('stops delivering and detaches the listener when the caller unsubscribes', async () => {
    const { listen, probe } = createListen()
    const callback = vi.fn()
    const client = createTauriTerminalPreviewClient(createInvoke(), listen)
    const unlisten = client.onData(callback)

    await Promise.resolve()
    unlisten()
    probe.emits(liveChunkTuple(1, 'late'))

    expect(callback).not.toHaveBeenCalled()
    expect(probe.unlisten).toHaveBeenCalledTimes(1)
  })

  it('detaches a post-unsubscribe settle so no late registration leaks', async () => {
    const unlisten = vi.fn()
    let settle!: (value: () => void) => void
    const listen = <T>(_event: string, _handler: (event: { payload: T }) => void) =>
      new Promise<() => void>((resolve) => {
        settle = resolve as unknown as (value: () => void) => void
      })
    const callback = vi.fn()
    const client = createTauriTerminalPreviewClient(createInvoke(), listen)

    const offData = client.onData(callback)
    offData()
    settle(unlisten)
    await Promise.resolve()
    await Promise.resolve()

    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('ignores events after unsubscribe even when the listener settled first', async () => {
    const { listen, probe } = createListen()
    const callback = vi.fn()
    const client = createTauriTerminalPreviewClient(createInvoke(), listen)
    const unlisten = client.onData(callback)
    // The subscription settled (handler registered), then the caller unsubscribed.
    await Promise.resolve()
    unlisten()
    probe.emits(liveChunkTuple(2, 'after-unsub'))

    expect(callback).not.toHaveBeenCalled()
  })
})
