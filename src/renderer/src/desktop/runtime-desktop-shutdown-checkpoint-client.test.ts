import assert from 'node:assert/strict'
import { it as test } from 'vitest'
import type { ShutdownCheckpointPayload } from '../../../shared/shutdown-checkpoint-types'
import { createRuntimeDesktopShutdownCheckpointClient } from './runtime-desktop-shutdown-checkpoint-client'

const checkpoint: ShutdownCheckpointPayload = {
  sessions: [],
  ui: { activeView: 'terminal' }
}

test('Electron keeps synchronous beforeunload staging and can await its durability boundary', async () => {
  const calls: string[] = []
  const client = createRuntimeDesktopShutdownCheckpointClient({
    isTauri: () => false,
    electron: {
      stageSync(value) {
        assert.equal(value, checkpoint)
        calls.push('stage')
      },
      async awaitDurable() {
        calls.push('durable')
      }
    },
    loadTauri: () => {
      throw new Error('Tauri client must not load in Electron')
    }
  })

  assert.equal(client.strategy, 'sync-beforeunload')
  client.stageSync(checkpoint)
  await client.stageAndWait(checkpoint)

  assert.deepEqual(calls, ['stage', 'stage', 'durable'])
})

test('Tauri versions and awaits the Rust-owned checkpoint command', async () => {
  const observed: unknown[] = []
  let tauriLoads = 0
  const client = createRuntimeDesktopShutdownCheckpointClient({
    isTauri: () => true,
    electron: {
      stageSync() {
        throw new Error('Electron staging must not run in Tauri')
      },
      async awaitDurable() {
        throw new Error('Electron durability wait must not run in Tauri')
      }
    },
    async loadTauri() {
      tauriLoads += 1
      return {
        async stageShutdownCheckpoint(value) {
          observed.push(value)
        }
      }
    }
  })

  assert.equal(client.strategy, 'async-close-guard')
  await client.stageAndWait(checkpoint)
  await client.stageAndWait(checkpoint)

  assert.equal(tauriLoads, 1)
  assert.deepEqual(observed, [
    { version: 1, ...checkpoint },
    { version: 1, ...checkpoint }
  ])
  assert.throws(() => client.stageSync(checkpoint), /cannot stage synchronously/i)
})
