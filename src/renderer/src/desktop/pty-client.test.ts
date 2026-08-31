import assert from 'node:assert/strict'
import { describe, expect, it as test, vi } from 'vitest'
import {
  PTY_DATA_EVENT,
  PTY_EXIT_EVENT,
  createTauriPtyClient,
  type PtyApi,
  type PtyDataPayload,
  type PtyExitPayload
} from './pty-client'
import { DesktopCommandError } from './desktop-client'

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

test('typed client maps one method onto each pty command with camelCase args', async () => {
  const { calls, invoke } = recordedInvoke({
    pty_spawn: () => ({ id: 1, shell: 'pwsh.exe' }),
    pty_write: () => null,
    pty_resize: () => null,
    pty_kill: () => null,
    pty_has_pty: () => true
  })
  const client = createTauriPtyClient(invoke, async () => () => undefined)

  const spawned = await client.spawn({
    cols: 80,
    rows: 24,
    cwd: 'D:/repo',
    shellOverride: 'C:/Program Files/PowerShell/pwsh.exe',
    env: { MCODE_TEST: '1' }
  })
  assert.deepEqual(spawned, { id: 1, shell: 'pwsh.exe' })
  await client.write(1, 'dir\r\n')
  await client.resize(1, 100, 30)
  await client.kill(1)
  assert.equal(await client.hasPty(1), true)

  assert.deepEqual(
    calls.map(({ command }) => command),
    ['pty_spawn', 'pty_write', 'pty_resize', 'pty_kill', 'pty_has_pty']
  )
  assert.deepEqual(calls[0]?.args, {
    spec: {
      cols: 80,
      rows: 24,
      cwd: 'D:/repo',
      shellOverride: 'C:/Program Files/PowerShell/pwsh.exe',
      env: { MCODE_TEST: '1' }
    }
  })
  assert.deepEqual(calls[1]?.args, { id: 1, data: 'dir\r\n' })
  assert.deepEqual(calls[2]?.args, { id: 1, cols: 100, rows: 30 })
  assert.deepEqual(calls[3]?.args, { id: 1 })
  assert.deepEqual(calls[4]?.args, { id: 1 })
})

test('spawn omits optional fields that were not provided', async () => {
  const { calls, invoke } = recordedInvoke({ pty_spawn: () => ({ id: 7, shell: 'cmd.exe' }) })
  const client = createTauriPtyClient(invoke, async () => () => undefined)

  const spawned = await client.spawn({ cols: 120, rows: 40 })
  assert.deepEqual(spawned, { id: 7, shell: 'cmd.exe' })
  assert.deepEqual(calls[0]?.args, { spec: { cols: 120, rows: 40 } })
})

test('invoke failures normalize onto DesktopCommandError with the native code', async () => {
  const client = createTauriPtyClient(
    async () => {
      throw {
        code: 'pty_input_too_large',
        context: { operation: 'pty_write', requestId: null, intent: null },
        message: 'terminal input is too large for a safe terminal send',
        userMessage: 'MCode could not send this input to the terminal. Try again.'
      }
    },
    async () => () => undefined
  )

  const error = await client
    .write(1, 'x'.repeat(17 * 1024 * 1024))
    .catch((caught: unknown) => caught)

  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'pty_input_too_large')
  assert.equal(error.context.operation, 'pty_write')
  assert.equal(error.userMessage, 'MCode could not send this input to the terminal. Try again.')
})

test('data subscription filters foreign payloads and unsubscription stops delivery', async () => {
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

  const dataEvents: PtyDataPayload[] = []
  const exitEvents: PtyExitPayload[] = []
  const unsubscribeData = await client.onData((payload) => dataEvents.push(payload))
  await client.onExit((payload) => exitEvents.push(payload))
  assert.equal(listeners.length, 2)

  const valid: PtyDataPayload = {
    id: 1,
    chunks: [{ data: 'hello', seq: 5, rawLength: undefined, transformed: undefined }]
  }
  listeners[0]?.({ payload: valid })
  listeners[0]?.({ payload: { unrelated: true } })
  listeners[1]?.({ payload: { id: 1, code: 0 } })
  listeners[1]?.({ payload: { id: 'nope' } })

  assert.deepEqual(dataEvents, [valid])
  assert.deepEqual(exitEvents, [{ id: 1, code: 0 }])

  unsubscribeData()
  assert.equal(unlistenCalls, 1)
})

test('the event names match the native emit contract', () => {
  assert.equal(PTY_DATA_EVENT, 'pty://data')
  assert.equal(PTY_EXIT_EVENT, 'pty://exit')
})

describe('smoke', () => {
  test('listener registration goes through the typed event source once per subscription', async () => {
    const listen = vi.fn(async () => () => undefined)
    const client: PtyApi = createTauriPtyClient(async () => null, listen)

    await client.onData(() => undefined)
    await client.onExit(() => undefined)

    expect(listen).toHaveBeenCalledTimes(2)
    expect(listen).toHaveBeenNthCalledWith(1, PTY_DATA_EVENT, expect.any(Function))
    expect(listen).toHaveBeenNthCalledWith(2, PTY_EXIT_EVENT, expect.any(Function))
  })
})
