import { describe, expect, it } from 'vitest'
import {
  createSnapshotPayloadCache,
  decodeTrailingUtf8
} from './terminal-scrollback-snapshot-cache'

describe('createSnapshotPayloadCache', () => {
  it('returns stored payloads and undefined for misses', () => {
    const cache = createSnapshotPayloadCache()
    expect(cache.get('missing')).toBeUndefined()
    cache.put('a', 'alpha')
    expect(cache.get('a')).toBe('alpha')
  })

  it('evicts the least recently inserted entry beyond maxEntries', () => {
    const cache = createSnapshotPayloadCache({ maxEntries: 2 })
    cache.put('a', 'one')
    cache.put('b', 'two')
    cache.put('c', 'three')
    expect(cache.get('a')).toBeUndefined()
    expect(cache.get('b')).toBe('two')
    expect(cache.get('c')).toBe('three')
  })

  it('promotes reads so touched entries survive insertion-pressure eviction', () => {
    const cache = createSnapshotPayloadCache({ maxEntries: 2 })
    cache.put('a', 'one')
    cache.put('b', 'two')
    expect(cache.get('a')).toBe('one')
    cache.put('c', 'three')
    expect(cache.get('a')).toBe('one')
    expect(cache.get('b')).toBeUndefined()
    expect(cache.get('c')).toBe('three')
  })

  it('keeps total UTF-8 bytes under maxBytes', () => {
    const cache = createSnapshotPayloadCache({ maxBytes: 10 })
    cache.put('big', 'x'.repeat(8))
    cache.put('small', 'y'.repeat(4))
    expect(cache.get('big')).toBeUndefined()
    expect(cache.get('small')).toBe('yyyy')
  })

  it('never evicts the just-written entry even when it alone exceeds the byte bound', () => {
    const cache = createSnapshotPayloadCache({ maxBytes: 4 })
    cache.put('huge', 'x'.repeat(64))
    expect(cache.get('huge')).toBe('x'.repeat(64))
  })

  it('replaces byte accounting on overwrite', () => {
    const cache = createSnapshotPayloadCache({ maxBytes: 12 })
    cache.put('k', 'x'.repeat(10))
    cache.put('k', 'y'.repeat(6))
    expect(cache.get('k')).toBe('yyyyyy')
    cache.put('other', 'z'.repeat(6))
    // 6 + 6 <= 12, so both fit once the overwritten 10 bytes were released.
    expect(cache.get('k')).toBe('yyyyyy')
    expect(cache.get('other')).toBe('zzzzzz')
  })

  it('invalidate removes the entry and releases its bytes', () => {
    const cache = createSnapshotPayloadCache({ maxBytes: 12 })
    cache.put('gone', 'x'.repeat(10))
    cache.invalidate('gone')
    expect(cache.get('gone')).toBeUndefined()
    cache.put('fresh', 'y'.repeat(10))
    expect(cache.get('fresh')).toBe('y'.repeat(10))
  })
})

describe('decodeTrailingUtf8', () => {
  it('decodes buffers that start on a codepoint boundary unchanged', () => {
    expect(decodeTrailingUtf8(Buffer.from('hello', 'utf-8'))).toBe('hello')
  })

  it('skips leading continuation bytes left by a mid-codepoint trailing slice', () => {
    // U+1F600 = F0 9F 98 80; slicing after the F0 leaves only continuation bytes.
    const sliced = Buffer.from([0x41, 0xf0, 0x9f, 0x98, 0x80]).subarray(2)
    expect(decodeTrailingUtf8(sliced)).toBe('')
  })

  it('keeps a trailing slice whose boundary landed before a complete sequence', () => {
    const base = Buffer.from('a\u{1F600}b', 'utf-8')
    // Slice starts exactly at the emoji's F0 lead byte: a complete sequence follows.
    const sliced = base.subarray(base.indexOf(0xf0))
    expect(decodeTrailingUtf8(sliced)).toBe('\u{1F600}b')
  })
})
