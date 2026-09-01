// Why: the Tauri replay channel is new renderer surface (PTY_CONTRACT_SPEC §9.3)
// — replay payloads validate like data/exit, and every pty command id crosses
// the invoke boundary as the decimal string the Rust pty_* commands parse.
import assert from 'node:assert/strict'
import { describe, expect, it as test, vi } from 'vitest'
import {
  PTY_DATA_EVENT,
  PTY_EXIT_EVENT,
  PTY_REPLAY_EVENT,
  createTauriPtyClient,
  type PtyApi,
  type PtyReplayPayload
} from './pty-client'

function recordedInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
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

test('replay subscription validates payloads and unsubscribes', async () => {
  const listeners: ((event: { payload: unknown }) => void)[] = []
  let unlistenCalls = 0
  const client = createTauriPtyClient(
    async () => null,
    async (_event, listener) => {
      listeners.push(listener)
      return () => {
        unlistenCalls += 1
      }
    }
  )

  const replayEvents: PtyReplayPayload[] = []
  const unsubscribeReplay = await client.onReplay((payload) => replayEvents.push(payload))
  assert.equal(listeners.length, 1)

  // Valid payload forwards; foreign shapes (wrong id type, wrong data type,
  // non-object) are filtered exactly like onData/onExit do.
  listeners[0]?.({ payload: { id: 5, data: 'scrollback' } })
  listeners[0]?.({ payload: { id: '5', data: 'string-id' } })
  listeners[0]?.({ payload: { id: 5, data: 42 } })
  listeners[0]?.({ payload: { unrelated: true } })
  listeners[0]?.({ payload: null })

  assert.deepEqual(replayEvents, [{ id: 5, data: 'scrollback' }])

  unsubscribeReplay()
  assert.equal(unlistenCalls, 1)
})

test('the replay event name matches the native emit contract', () => {
  assert.equal(PTY_REPLAY_EVENT, 'pty://replay')
  assert.equal(PTY_DATA_EVENT, 'pty://data')
  assert.equal(PTY_EXIT_EVENT, 'pty://exit')
})

test('every pty command sends its id as a decimal string at the invoke boundary', async () => {
  const { calls, invoke } = recordedInvoke({
    pty_write: () => null,
    pty_resize: () => null,
    pty_kill: () => null,
    pty_has_pty: () => true,
    pty_ack_data: () => null,
    pty_get_size: () => ({ cols: 80, rows: 24 })
  })
  const client = createTauriPtyClient(invoke, async () => () => undefined)

  await client.write(12, 'ls\r\n')
  await client.resize(12, 100, 30)
  await client.kill(12)
  assert.equal(await client.hasPty(12), true)
  await client.ackData(12, 5)
  assert.deepEqual(await client.getSize(12), { cols: 80, rows: 24 })

  assert.deepEqual(calls[0]?.args, { id: '12', data: 'ls\r\n' })
  assert.deepEqual(calls[1]?.args, { id: '12', cols: 100, rows: 30 })
  assert.deepEqual(calls[2]?.args, { id: '12' })
  assert.deepEqual(calls[3]?.args, { id: '12' })
  assert.deepEqual(calls[4]?.args, { id: '12', processedChars: 5 })
  assert.deepEqual(calls[5]?.args, { id: '12' })
})

describe('smoke', () => {
  test('listener registration goes through the typed event source once per subscription', async () => {
    const listen = vi.fn(async () => () => undefined)
    const client: PtyApi = createTauriPtyClient(async () => null, listen)

    await client.onReplay(() => undefined)

    expect(listen).toHaveBeenCalledTimes(1)
    expect(listen).toHaveBeenCalledWith(PTY_REPLAY_EVENT, expect.any(Function))
  })
})
