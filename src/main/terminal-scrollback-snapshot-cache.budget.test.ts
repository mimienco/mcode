import { describe, expect, it } from 'vitest'
import {
  createSnapshotPayloadCache,
  decodeTrailingUtf8
} from './terminal-scrollback-snapshot-cache'

// PERF-13 stage A: the warmup must stop before evicting warmed entries, and
// priority refs must survive a budget stop.
describe('snapshot payload cache budget', () => {
  it('reports capacity so callers can stop before eviction', () => {
    const cache = createSnapshotPayloadCache({ maxEntries: 2, maxBytes: 1024 })
    expect(cache.hasCapacityFor(512)).toBe(true)
    cache.put('a', 'x'.repeat(400))
    cache.put('b', 'x'.repeat(400))
    // Full by count: another put would evict 'a'.
    expect(cache.hasCapacityFor(1)).toBe(false)
  })

  it('stops accepting when the byte budget would overflow', () => {
    const cache = createSnapshotPayloadCache({ maxEntries: 10, maxBytes: 100 })
    cache.put('big', 'x'.repeat(90))
    expect(cache.hasCapacityFor(20)).toBe(false)
    expect(cache.hasCapacityFor(5)).toBe(true)
  })

  it('keeps replay truncation semantics on cached payloads', () => {
    const cache = createSnapshotPayloadCache()
    const payload = decodeTrailingUtf8(Buffer.from('hello'))
    cache.put('k', payload)
    expect(cache.get('k')).toBe('hello')
  })
})
