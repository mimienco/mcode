import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { it as test } from 'vitest'
import type { ShutdownCheckpoint } from '../../../shared/shutdown-checkpoint-types'
import type { WorkspaceSessionState } from '../../../shared/workspace-session-state-types'
import {
  createDesktopClient,
  createDesktopInvoke,
  DesktopCommandError,
  isShutdownRequest,
  type DesktopAppLifecycleCapability,
  type DesktopWindowCapability
} from './desktop-client'

function createWindowCapability(
  overrides: Partial<DesktopWindowCapability> = {}
): DesktopWindowCapability {
  return {
    minimize: () => Promise.resolve(),
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onMaximizedChanged: () => Promise.resolve(() => undefined),
    requestClose: () => Promise.resolve(),
    onCloseRequested: () => Promise.resolve(() => undefined),
    cancelClose: () => Promise.resolve(undefined),
    confirmClose: () => Promise.resolve(),
    ...overrides
  }
}

test('shutdown request fixture matches the TypeScript contract', () => {
  const fixture = JSON.parse(
    readFileSync(resolve('crates/mcode-app/tests/fixtures/shutdown-request-v1.json'), 'utf8')
  ) as unknown

  assert.equal(isShutdownRequest(fixture), true)
})

function createAppLifecycleCapability(): DesktopAppLifecycleCapability {
  return {
    requestShutdown: (intent) => Promise.resolve({ version: 1, requestId: '1', intent }),
    onShutdownRequested: () => Promise.resolve(() => undefined),
    cancelShutdown: () => Promise.resolve(),
    confirmAndExecuteShutdown: () => Promise.resolve()
  }
}

test('desktop invoke normalizes structured backend rejections into a typed error', async () => {
  const payload = {
    code: 'stale_shutdown_request',
    context: {
      operation: 'app_confirm_and_execute_shutdown',
      requestId: '42',
      intent: 'restart' as const
    },
    message: 'shutdown request does not match the pending request',
    userMessage: 'MCode could not complete the shutdown request. Try again.'
  }
  const invoke = createDesktopInvoke(async () => {
    throw payload
  })

  const error = await invoke('app_confirm_and_execute_shutdown', {
    requestId: '42'
  }).catch((caught: unknown) => caught)

  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, payload.code)
  assert.deepEqual(error.context, payload.context)
  assert.equal(error.message, payload.message)
  assert.equal(error.userMessage, payload.userMessage)
})

test('desktop invoke gives unknown native rejections a safe typed fallback', async () => {
  const invoke = createDesktopInvoke(async () => {
    throw new Error('transport failed')
  })

  const error = await invoke('app_get_launch_context').catch((caught: unknown) => caught)

  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'desktop_command_failed')
  assert.deepEqual(error.context, { operation: 'app_get_launch_context' })
  assert.equal(error.message, 'transport failed')
  assert.equal(error.userMessage, 'MCode could not complete the desktop request. Try again.')
})

test('launch context uses the narrow Tauri command and preserves its typed result', async () => {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const expected = {
    version: '1.4.178',
    buildLane: 'development' as const,
    headless: false,
    safeMode: false
  }
  const client = createDesktopClient(
    async (command, args) => {
      calls.push({ command, args })
      return expected
    },
    createWindowCapability(),
    createAppLifecycleCapability()
  )

  const context = await client.app.getLaunchContext()

  assert.deepEqual(calls, [{ command: 'app_get_launch_context', args: undefined }])
  assert.deepEqual(context, expected)
})

test('shutdown checkpoint uses the versioned narrow Tauri command', async () => {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const checkpoint: ShutdownCheckpoint = {
    version: 1 as const,
    sessions: [{ state: { activeWorktreeId: 'worktree-1' } as WorkspaceSessionState }],
    ui: { activeView: 'terminal' }
  }
  const client = createDesktopClient(
    async (command, args) => {
      calls.push({ command, args })
      return undefined
    },
    createWindowCapability(),
    createAppLifecycleCapability()
  )

  await client.app.stageShutdownCheckpoint(checkpoint)

  assert.deepEqual(calls, [{ command: 'app_stage_shutdown_checkpoint', args: { checkpoint } }])
})

test('window minimize delegates through the desktop window capability', async () => {
  let minimizeCalls = 0
  const client = createDesktopClient(
    async () => undefined,
    createWindowCapability({
      async minimize() {
        minimizeCalls += 1
      }
    }),
    createAppLifecycleCapability()
  )

  await client.window.minimize()

  assert.equal(minimizeCalls, 1)
})

test('window maximize toggle delegates through the desktop window capability', async () => {
  let toggleCalls = 0
  const client = createDesktopClient(
    async () => undefined,
    createWindowCapability({
      async toggleMaximize() {
        toggleCalls += 1
      }
    }),
    createAppLifecycleCapability()
  )

  await client.window.toggleMaximize()

  assert.equal(toggleCalls, 1)
})

test('window maximized state comes from the desktop window capability', async () => {
  const client = createDesktopClient(
    async () => undefined,
    createWindowCapability({
      isMaximized: () => Promise.resolve(true)
    }),
    createAppLifecycleCapability()
  )

  assert.equal(await client.window.isMaximized(), true)
})

test('window maximized subscription delegates listener ownership and cleanup', async () => {
  const observed: boolean[] = []
  let cleanedUp = false
  const client = createDesktopClient(
    async () => undefined,
    createWindowCapability({
      async onMaximizedChanged(listener) {
        listener(true)
        return () => {
          cleanedUp = true
        }
      }
    }),
    createAppLifecycleCapability()
  )

  const unsubscribe = await client.window.onMaximizedChanged((value) => observed.push(value))
  unsubscribe()

  assert.deepEqual(observed, [true])
  assert.equal(cleanedUp, true)
})

test('window close request delegates through the desktop window capability', async () => {
  let closeRequests = 0
  const client = createDesktopClient(
    async () => undefined,
    createWindowCapability({
      async requestClose() {
        closeRequests += 1
      }
    }),
    createAppLifecycleCapability()
  )

  await client.window.requestClose()

  assert.equal(closeRequests, 1)
})
