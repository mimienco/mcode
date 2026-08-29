import assert from 'node:assert/strict'
import { it as test } from 'vitest'
import type { ShutdownCheckpoint } from '../../../shared/shutdown-checkpoint-types'
import type { WorkspaceSessionState } from '../../../shared/workspace-session-state-types'
import { createDesktopClient, type DesktopWindowCapability } from './desktop-client'

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
    confirmClose: () => Promise.resolve(),
    ...overrides
  }
}

test('launch context uses the narrow Tauri command and preserves its typed result', async () => {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const expected = {
    version: '1.4.178',
    buildLane: 'development' as const,
    headless: false,
    safeMode: false
  }
  const client = createDesktopClient(async (command, args) => {
    calls.push({ command, args })
    return expected
  }, createWindowCapability())

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
  const client = createDesktopClient(async (command, args) => {
    calls.push({ command, args })
    return undefined
  }, createWindowCapability())

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
    })
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
    })
  )

  await client.window.toggleMaximize()

  assert.equal(toggleCalls, 1)
})

test('window maximized state comes from the desktop window capability', async () => {
  const client = createDesktopClient(
    async () => undefined,
    createWindowCapability({
      isMaximized: () => Promise.resolve(true)
    })
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
    })
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
    })
  )

  await client.window.requestClose()

  assert.equal(closeRequests, 1)
})
