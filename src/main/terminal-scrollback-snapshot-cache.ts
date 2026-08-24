/**
 * Process-local LRU cache for terminal scrollback snapshot payloads, plus the
 * payload derivation helper shared with the disk read path.
 *
 * Why: reveal-time scrollback hydration uses synchronous IPC
 * (`session:read-terminal-scrollback-sync`) so a remounting pane can paint its
 * prior buffer inside the activation frame. On a cold answer main runs
 * statSync + readSync of up to TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT per leaf,
 * stalling every queued IPC — the multi-second workspace-tab-switch freeze.
 * Write-through caching keeps the synchronous contract intact while repeat
 * reads (park -> return, StrictMode remounts) never touch the disk.
 *
 * Bounds: entries are capped by count and by total UTF-8 byte length. Every
 * cached payload is already replay-truncated (<= 512KiB), so the byte bound
 * cannot deadlock on an oversized single entry.
 */

const DEFAULT_MAX_ENTRIES = 48
const DEFAULT_MAX_BYTES = 8 * 1024 * 1024

export type SnapshotPayloadCache = {
  get: (key: string) => string | undefined
  put: (key: string, payload: string) => void
  invalidate: (key: string) => void
  /** Why: PERF-13 — lets the warmup stop before it would evict warmed entries. */
  hasCapacityFor: (bytes: number) => boolean
}

export function createSnapshotPayloadCache(options?: {
  maxEntries?: number
  maxBytes?: number
}): SnapshotPayloadCache {
  const maxEntries = options?.maxEntries ?? DEFAULT_MAX_ENTRIES
  const maxBytes = options?.maxBytes ?? DEFAULT_MAX_BYTES
  const entries = new Map<string, string>()
  let totalBytes = 0

  return {
    get: (key) => {
      const hit = entries.get(key)
      if (hit === undefined) {
        return undefined
      }
      // Why: Map iterates in insertion order, so delete+set promotes the key to most-recently used.
      entries.delete(key)
      entries.set(key, hit)
      return hit
    },
    put: (key, payload) => {
      const previous = entries.get(key)
      if (previous !== undefined) {
        totalBytes -= Buffer.byteLength(previous, 'utf-8')
        entries.delete(key)
      }
      entries.set(key, payload)
      totalBytes += Buffer.byteLength(payload, 'utf-8')
      while (entries.size > 0 && (totalBytes > maxBytes || entries.size > maxEntries)) {
        const oldestKey = entries.keys().next().value
        // Why: stop once the just-written entry would itself be evicted; keeping it beats an empty cache.
        if (oldestKey === undefined || oldestKey === key) {
          break
        }
        const oldest = entries.get(oldestKey)
        entries.delete(oldestKey)
        if (oldest !== undefined) {
          totalBytes -= Buffer.byteLength(oldest, 'utf-8')
        }
      }
    },
    invalidate: (key) => {
      const removed = entries.get(key)
      if (removed !== undefined) {
        totalBytes -= Buffer.byteLength(removed, 'utf-8')
        entries.delete(key)
      }
    },
    hasCapacityFor: (bytes) => entries.size + 1 <= maxEntries && totalBytes + bytes <= maxBytes
  }
}

/** The process-wide cache backing the synchronous snapshot read path. */
export const terminalScrollbackSnapshotCache = createSnapshotPayloadCache()

/**
 * Decode a buffer whose first byte may land mid-UTF-8-sequence (trailing-byte
 * slices of larger payloads). Mirrors the leading-continuation skip that the
 * disk reader has always applied, so cached derivations stay byte-exact with
 * what a fresh read of the same file returns.
 */
export function decodeTrailingUtf8(buffer: Buffer): string {
  let start = 0
  while (start < buffer.length && (buffer[start] & 0xc0) === 0x80) {
    start++
  }
  return buffer.subarray(start).toString('utf-8')
}
