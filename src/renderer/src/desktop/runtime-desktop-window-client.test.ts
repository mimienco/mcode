import assert from 'node:assert/strict'
import { it as test } from 'vitest'
import type { DesktopClient } from './desktop-client'
import { createRuntimeDesktopWindowClient } from './runtime-desktop-window-client'

type DesktopWindowClient = DesktopClient['window']

function createWindowClient(minimize: () => Promise<void>): DesktopWindowClient {
  return {
    minimize,
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onMaximizedChanged: () => Promise.resolve(() => undefined),
    requestClose: () => Promise.resolve(),
    onCloseRequested: () => Promise.resolve(() => undefined),
    confirmClose: () => Promise.resolve()
  }
}

test('Electron runtime routes window behavior without loading the Tauri client', async () => {
  let electronMinimizeCalls = 0
  let tauriLoads = 0
  const client = createRuntimeDesktopWindowClient({
    isTauri: () => false,
    electron: createWindowClient(async () => {
      electronMinimizeCalls += 1
    }),
    async loadTauri() {
      tauriLoads += 1
      return createWindowClient(() => Promise.resolve())
    }
  })

  await client.minimize()

  assert.equal(electronMinimizeCalls, 1)
  assert.equal(tauriLoads, 0)
})

test('Tauri runtime loads one native client and reuses it for window behavior', async () => {
  let tauriLoads = 0
  let tauriMinimizeCalls = 0
  const client = createRuntimeDesktopWindowClient({
    isTauri: () => true,
    electron: createWindowClient(() => {
      throw new Error('Electron window client must not run in Tauri')
    }),
    async loadTauri() {
      tauriLoads += 1
      return createWindowClient(async () => {
        tauriMinimizeCalls += 1
      })
    }
  })

  await client.minimize()
  await client.minimize()

  assert.equal(tauriLoads, 1)
  assert.equal(tauriMinimizeCalls, 2)
})
