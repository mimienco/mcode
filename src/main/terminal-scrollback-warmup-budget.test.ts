import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

// PERF-13 stage A: warmup must stop before evicting warmed entries and must
// order priority refs (shutdown-active tab) first. Structural pins keep the
// wiring honest without a full Electron harness.
function readSource(relativePath: string): string {
  return readFileSync(join(process.cwd(), relativePath), 'utf8').replace(/\r\n/g, '\n')
}

const SNAPSHOTS_PATH = 'src/main/terminal-scrollback-snapshot-warmup.ts'
const STORE_PATH = 'src/main/persistence/loading-store/store.ts'
const CACHE_PATH = 'src/main/terminal-scrollback-snapshot-cache.ts'

describe('scrollback snapshot warmup budget', () => {
  it('stops warming before the cache would evict warmed entries', () => {
    const source = readSource(SNAPSHOTS_PATH)
    const warmupStart = source.indexOf('export async function warmTerminalScrollbackSnapshotCache')
    const warmup = source.slice(warmupStart, source.indexOf('}', source.indexOf('return warmed')))
    expect(warmup).toContain('hasCapacityFor')
    expect(warmup).toMatch(/if \(!snapshotPayloadCache\.hasCapacityFor\([^)]*\)\) \{\s*\n\s*break/)
  })

  it('warms the shutdown-active tab refs before everything else', () => {
    const snapshots = readSource(SNAPSHOTS_PATH)
    expect(snapshots).toContain('collectActiveTabScrollbackSnapshotRefs')
    expect(snapshots.indexOf('collectActiveTabScrollbackSnapshotRefs')).toBeGreaterThan(-1)

    const store = readSource(STORE_PATH)
    expect(store).toContain('priorityRefs: collectActiveTabScrollbackSnapshotRefs')
  })

  it('orders priority refs ahead of the remaining refs', () => {
    const source = readSource(SNAPSHOTS_PATH)
    const priorityFirst = source.indexOf('allRefs.filter((ref) => prioritySet.has(ref))')
    const restAfter = source.indexOf('allRefs.filter((ref) => !prioritySet.has(ref))')
    expect(priorityFirst).toBeGreaterThan(-1)
    expect(restAfter).toBeGreaterThan(priorityFirst)
  })

  it('exposes the cache budget API used by the warmup stop', () => {
    const cache = readSource(CACHE_PATH)
    expect(cache).toContain('hasCapacityFor')
  })
})
