import { join } from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  createSerializeSplitClient,
  resolveStateSerializeWorkerPath
} from './state-serialize-split-client'

// PERF-08 commit 2b: the offload client must time out and surface failures so
// the caller can fall back to the synchronous split; path resolution mirrors
// hang-watchdog's dev/packaged two-location scheme.
describe('serialize split client', () => {
  const escaped = ['"mcode-secret-slot-1"', '"mcode-secret-slot-2"']

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('resolves the adjacent build output in dev', () => {
    const existsSync = vi.fn((candidate: string) =>
      candidate.endsWith(join('/repo/out/main', 'state-serialize-worker-entry.js'))
    )
    expect(resolveStateSerializeWorkerPath('/repo/out/main', false, existsSync as never)).toBe(
      join('/repo/out/main', 'state-serialize-worker-entry.js')
    )
  })

  it('falls back to the packaged out/main location when dev output is absent', () => {
    const existsSync = vi.fn(
      (candidate: string) =>
        candidate === join('/app', 'out', 'main', 'state-serialize-worker-entry.js')
    )
    expect(resolveStateSerializeWorkerPath('/app', true, existsSync as never)).toBe(
      join('/app', 'out', 'main', 'state-serialize-worker-entry.js')
    )
  })

  it('returns null when no worker entry exists anywhere', () => {
    expect(resolveStateSerializeWorkerPath('/app', true, () => false)).toBeNull()
  })

  it('times out a stuck worker so the caller can fall back to sync', async () => {
    const terminate = vi.fn()
    const client = createSerializeSplitClient(
      {
        resolveWorkerPath: () => '/worker.js',
        spawnWorker: () => ({
          post: () => {},
          terminate,
          onResult: () => {}
        })
      },
      { timeoutMs: 20 }
    )

    await expect(client.split('{"a":1}', escaped)).rejects.toThrow('timed out')
    expect(terminate).toHaveBeenCalled()
  })
})
