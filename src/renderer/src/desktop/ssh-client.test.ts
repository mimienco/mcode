import assert from 'node:assert/strict'
import { describe, expect, it as test, vi } from 'vitest'
import { createTauriSshClient, type SshConnectResponse, type SshTargetDto } from './ssh-client'
import { DesktopCommandError } from './desktop-client'

function recordedInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler(args ?? {})
  }
  return { calls, invoke }
}

test('connect sends the target under the camelCase key and parses the connected shape', async () => {
  const target: SshTargetDto = {
    host: 'example.internal',
    port: 22,
    username: 'deploy',
    auth: { type: 'key', keyPath: '~/.ssh/id_ed25519' },
    connectTimeoutMs: 15_000
  }
  const { calls, invoke } = recordedInvoke({
    ssh_connect: () => ({ status: 'connected', connectionId: 7 })
  })
  const client = createTauriSshClient(invoke)

  const response = await client.connect(target)

  assert.deepEqual(calls, [{ command: 'ssh_connect', args: { target } }])
  assert.deepEqual(response, { status: 'connected', connectionId: 7 })
})

test('host-key-unknown response parses with the presented key fields', async () => {
  const presented: SshConnectResponse = {
    status: 'host-key-unknown',
    connectAttemptId: 42,
    fingerprint: 'SHA256:0123456789abcdef',
    algorithm: 'ssh-ed25519',
    keyBase64: 'AAAAC3NzaC1lZDI1NTE5AAAAI…'
  }
  const { invoke } = recordedInvoke({ ssh_connect: () => presented })
  const client = createTauriSshClient(invoke)

  const response = await client.connect({
    host: 'example.internal',
    port: 22,
    username: 'deploy',
    auth: { type: 'password', password: 'hunter2' }
  })

  assert.equal(response.status, 'host-key-unknown')
  assert.equal(response.connectAttemptId, 42)
  assert.equal(response.fingerprint, 'SHA256:0123456789abcdef')
  assert.equal(response.algorithm, 'ssh-ed25519')
  assert.equal(response.keyBase64, 'AAAAC3NzaC1lZDI1NTE5AAAAI…')
})

test('acceptHostKey sends accept:false and parses the rejected shape', async () => {
  const { calls, invoke } = recordedInvoke({ ssh_accept_host_key: () => ({ status: 'rejected' }) })
  const client = createTauriSshClient(invoke)

  const response = await client.acceptHostKey(42, false)

  assert.deepEqual(calls, [
    { command: 'ssh_accept_host_key', args: { connectAttemptId: 42, accept: false } }
  ])
  assert.deepEqual(response, { status: 'rejected' })
})

test('exec sends camelCase args and clamps timeoutMs into 1000..600000', async () => {
  const { calls, invoke } = recordedInvoke({
    ssh_exec: () => ({ stdout: 'out', stderr: '', exitCode: 0 })
  })
  const client = createTauriSshClient(invoke)

  await client.exec(7, 'uptime')
  await client.exec(7, 'sleep 1', 5_000)
  await client.exec(7, 'too-small', 50)
  await client.exec(7, 'too-big', 999_999)

  assert.deepEqual(calls[0]?.args, { connectionId: 7, command: 'uptime', timeoutMs: 120_000 })
  assert.deepEqual(calls[1]?.args, { connectionId: 7, command: 'sleep 1', timeoutMs: 5_000 })
  assert.deepEqual(calls[2]?.args, { connectionId: 7, command: 'too-small', timeoutMs: 1_000 })
  assert.deepEqual(calls[3]?.args, { connectionId: 7, command: 'too-big', timeoutMs: 600_000 })
})

test('pty methods map onto their ssh_* commands with camelCase args', async () => {
  const { calls, invoke } = recordedInvoke({
    ssh_open_pty: () => ({ ptyId: '2:pty-1' }),
    ssh_write_pty: () => undefined,
    ssh_resize_pty: () => undefined,
    ssh_close_pty: () => undefined,
    ssh_disconnect: () => undefined
  })
  const client = createTauriSshClient(invoke)

  const opened = await client.openPty(2, 120, 40)
  await client.writePty('2:pty-1', 'ls\r\n')
  await client.resizePty('2:pty-1', 100, 30)
  await client.closePty('2:pty-1')
  await client.disconnect(2)

  assert.deepEqual(
    calls.map(({ command }) => command),
    ['ssh_open_pty', 'ssh_write_pty', 'ssh_resize_pty', 'ssh_close_pty', 'ssh_disconnect']
  )
  assert.deepEqual(calls[0]?.args, { connectionId: 2, cols: 120, rows: 40 })
  assert.deepEqual(opened, { ptyId: '2:pty-1' })
  assert.deepEqual(calls[1]?.args, { ptyId: '2:pty-1', data: 'ls\r\n' })
  assert.deepEqual(calls[2]?.args, { ptyId: '2:pty-1', cols: 100, rows: 30 })
  assert.deepEqual(calls[3]?.args, { ptyId: '2:pty-1' })
  assert.deepEqual(calls[4]?.args, { connectionId: 2 })
})

test('invoke failures normalize onto DesktopCommandError with the native code', async () => {
  const { invoke } = recordedInvoke({
    ssh_exec: () => {
      throw {
        code: 'ssh_connection_gone',
        context: { operation: 'ssh_exec', connectionId: 7, detail: null },
        message: 'SSH exec failed',
        userMessage: 'MCode could not run the SSH command. Try again.'
      }
    }
  })
  const client = createTauriSshClient(invoke)

  const rejection = await client.exec(7, 'uptime').then(
    () => null,
    (error: unknown) => error
  )
  assert.ok(rejection instanceof DesktopCommandError)
  assert.equal(rejection.code, 'ssh_connection_gone')
  assert.equal(rejection.context.operation, 'ssh_exec')
  assert.equal(rejection.userMessage, 'MCode could not run the SSH command. Try again.')
})

describe('smoke', () => {
  test('the client is constructible over a minimal transport', async () => {
    const client = createTauriSshClient(async () => ({ status: 'rejected' }))
    expect(vi.isMockFunction(client.connect)).toBe(false)
  })
})
