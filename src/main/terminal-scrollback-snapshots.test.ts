import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { writeFileSync, unlinkSync, readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

vi.mock('electron', () => ({
  app: {
    getPath: () => tmpdir()
  }
}))

import {
  deleteTerminalScrollbackSnapshot,
  deleteTerminalScrollbackSnapshotSync,
  makeTerminalScrollbackSnapshotRef,
  readTerminalScrollbackSnapshotSync,
  writeTerminalScrollbackSnapshot,
  writeTerminalScrollbackSnapshotSync
} from './terminal-scrollback-snapshots'
import { TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT } from '../shared/terminal-scrollback-limits'

let snapshotRoot = ''
let counter = 0

// Why longhand: this is the independent oracle for what a cold disk read returns;
// reusing the module's helpers here would make the cache-parity assertion circular.
function expectedDiskPayload(file: string): string {
  const bytes = readFileSync(file)
  const length = Math.min(bytes.length, TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT)
  const trailing = bytes.subarray(bytes.length - length)
  let start = 0
  while (start < trailing.length && (trailing[start] & 0xc0) === 0x80) {
    start++
  }
  return trailing.subarray(start).toString('utf-8')
}

beforeEach(() => {
  snapshotRoot = mkdtempSync(join(tmpdir(), 'mcode-scrollback-cache-'))
  counter += 1
})

afterEach(() => {
  rmSync(snapshotRoot, { recursive: true, force: true })
})

describe('scrollback snapshot payload cache', () => {
  it('serves a sync-written snapshot from memory after the file is removed', () => {
    const tabId = `tabsync-${counter}`
    const ref = writeTerminalScrollbackSnapshotSync({
      tabId,
      leafId: 'leaf-a',
      buffer: 'hello cached world',
      storage: { snapshotRoot }
    })
    expect(ref).not.toBeNull()
    const file = join(snapshotRoot, `${ref}.bin`)
    expect(() => unlinkSync(file)).not.toThrow()
    expect(readTerminalScrollbackSnapshotSync(ref!, { snapshotRoot })).toBe('hello cached world')
  })

  it('caches the exact bytes a cold disk read would return for oversized buffers', () => {
    const tabId = `tabbig-${counter}`
    // Store limit (5MiB) >> replay limit; fill enough ASCII that the replay slice starts mid-buffer.
    const marker = 'END-MARKER-π'
    const buffer = `${'x'.repeat(TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT + 4096)}${marker}`
    const ref = writeTerminalScrollbackSnapshotSync({
      tabId,
      leafId: 'leaf-big',
      buffer,
      storage: { snapshotRoot }
    })!
    const file = join(snapshotRoot, `${ref}.bin`)
    const expected = expectedDiskPayload(file)
    expect(expected.length).toBeLessThanOrEqual(TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT)
    expect(expected.endsWith(marker)).toBe(true)
    unlinkSync(file)
    expect(readTerminalScrollbackSnapshotSync(ref, { snapshotRoot })).toBe(expected)
  })

  it('serves an async-written snapshot from memory and invalidates on async delete', async () => {
    const tabId = `tabasync-${counter}`
    const ref = await writeTerminalScrollbackSnapshot({
      tabId,
      leafId: 'leaf-async',
      buffer: 'async payload',
      storage: { snapshotRoot }
    })
    expect(ref).not.toBeNull()
    unlinkSync(join(snapshotRoot, `${ref}.bin`))
    expect(readTerminalScrollbackSnapshotSync(ref!, { snapshotRoot })).toBe('async payload')
    await deleteTerminalScrollbackSnapshot(ref!, { snapshotRoot })
    expect(readTerminalScrollbackSnapshotSync(ref!, { snapshotRoot })).toBeNull()
  })

  it('overwrite refreshes the cached payload', () => {
    const tabId = `tabow-${counter}`
    const args = { tabId, leafId: 'leaf-ow', storage: { snapshotRoot } as const }
    writeTerminalScrollbackSnapshotSync({ ...args, buffer: 'first' })
    writeTerminalScrollbackSnapshotSync({ ...args, buffer: 'second' })
    const ref = makeTerminalScrollbackSnapshotRef(tabId, 'leaf-ow')
    expect(readTerminalScrollbackSnapshotSync(ref, { snapshotRoot })).toBe('second')
  })

  it('sync delete invalidates so a missing file reads as null', () => {
    const tabId = `tabdel-${counter}`
    const ref = writeTerminalScrollbackSnapshotSync({
      tabId,
      leafId: 'leaf-del',
      buffer: 'doomed',
      storage: { snapshotRoot }
    })!
    deleteTerminalScrollbackSnapshotSync(ref, { snapshotRoot })
    expect(readTerminalScrollbackSnapshotSync(ref, { snapshotRoot })).toBeNull()
  })

  it('warm-up populates the cache for persisted refs without touching the file afterwards', async () => {
    const tabId = `tabwarm-${counter}`
    const ref = writeTerminalScrollbackSnapshotSync({
      tabId,
      leafId: 'leaf-warm',
      buffer: 'warmed bytes',
      storage: { snapshotRoot }
    })!
    // Simulate a fresh process: drop the just-written cache entry via sync delete+rewrite dance.
    deleteTerminalScrollbackSnapshotSync(ref, { snapshotRoot })
    writeFileSync(join(snapshotRoot, `${ref}.bin`), Buffer.from('warmed bytes', 'utf-8'))
    const warmed = await warmTerminalScrollbackSnapshotCache(
      {
        terminalLayoutsByTabId: {
          [tabId]: { scrollbackRefsByLeafId: { 'leaf-warm': ref } }
        }
      } as never,
      { snapshotRoot }
    )
    expect(warmed).toBe(1)
    unlinkSync(join(snapshotRoot, `${ref}.bin`))
    expect(readTerminalScrollbackSnapshotSync(ref, { snapshotRoot })).toBe('warmed bytes')
  })

  it('warm-up skips already-cached refs and warms only cold on-disk refs', async () => {
    const tabId = `tabskip-${counter}`
    const cachedRef = writeTerminalScrollbackSnapshotSync({
      tabId,
      leafId: 'leaf-cached',
      buffer: 'already cached',
      storage: { snapshotRoot }
    })!
    // A ref whose bytes exist on disk but were never written through this process's cache.
    const coldRef = makeTerminalScrollbackSnapshotRef(tabId, 'leaf-cold')
    writeFileSync(join(snapshotRoot, `${coldRef}.bin`), Buffer.from('cold bytes', 'utf-8'))
    const warmed = await warmTerminalScrollbackSnapshotCache(
      {
        terminalLayoutsByTabId: {
          [tabId]: {
            scrollbackRefsByLeafId: { 'leaf-cached': cachedRef, 'leaf-cold': coldRef }
          }
        }
      } as never,
      { snapshotRoot }
    )
    expect(warmed).toBe(1)
    expect(readTerminalScrollbackSnapshotSync(cachedRef, { snapshotRoot })).toBe('already cached')
    unlinkSync(join(snapshotRoot, `${coldRef}.bin`))
    expect(readTerminalScrollbackSnapshotSync(coldRef, { snapshotRoot })).toBe('cold bytes')
  })
})

import { warmTerminalScrollbackSnapshotCache } from './terminal-scrollback-snapshot-warmup'
