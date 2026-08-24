// Why: PERF-13 — warming belongs beside its own budget logic rather than inside
// the already-large snapshot read/write module.
import { readFile } from 'node:fs/promises'
import type { WorkspaceSessionState } from '../shared/workspace-session-state-types'
import type { TerminalScrollbackSnapshotStorage } from './terminal-scrollback-snapshots'
import { TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT } from '../shared/terminal-scrollback-limits'
import { terminalScrollbackSnapshotCache as snapshotPayloadCache } from './terminal-scrollback-snapshot-cache'
import {
  collectTerminalScrollbackSnapshotRefs,
  snapshotCacheKey,
  snapshotReadPaths,
  trailingReplayString
} from './terminal-scrollback-snapshots'

/** Why: PERF-13 — the pane the session was shut down on is the likeliest first
 * reveal after restart; its refs warm before every other tab's. */
export function collectActiveTabScrollbackSnapshotRefs(
  session: WorkspaceSessionState
): Set<string> {
  const activeLayout = session.activeTabId
    ? (session.terminalLayoutsByTabId ?? {})[session.activeTabId]
    : undefined
  return new Set(Object.values(activeLayout?.scrollbackRefsByLeafId ?? {}))
}

// Why: after a restart every persisted leaf carries a scrollback ref but the process-local
// cache is cold, so the first reveal of each tab paid a synchronous disk read inside the
// activation frame. Warming once from the freshly-loaded session moves those bytes onto
// async fs before any pane mounts; misses simply fall back to today's disk-read behavior.
export async function warmTerminalScrollbackSnapshotCache(
  session: WorkspaceSessionState | undefined,
  storage?: TerminalScrollbackSnapshotStorage,
  options?: { priorityRefs?: Iterable<string> }
): Promise<number> {
  if (!session) {
    return 0
  }
  // Why: PERF-13 — most-recently-active tabs warm first so a budget stop never
  // evicts-or-skips the panes the user is most likely to reveal next.
  const prioritySet = new Set(options?.priorityRefs ?? [])
  const allRefs = [...collectTerminalScrollbackSnapshotRefs(session)]
  const orderedRefs = [
    ...allRefs.filter((ref) => prioritySet.has(ref)),
    ...allRefs.filter((ref) => !prioritySet.has(ref))
  ]
  let warmed = 0
  for (const ref of orderedRefs) {
    const cacheKey = snapshotCacheKey(ref, storage)
    if (!cacheKey || snapshotPayloadCache.get(cacheKey) !== undefined) {
      continue
    }
    // Why: stop before the next read could evict an already-warmed entry —
    // thrashing the LRU here is what forced later reveals back onto sync I/O.
    if (!snapshotPayloadCache.hasCapacityFor(TERMINAL_SCROLLBACK_REPLAY_BYTE_LIMIT)) {
      break
    }
    for (const path of snapshotReadPaths(ref, storage)) {
      try {
        const bytes = await readFile(path)
        snapshotPayloadCache.put(cacheKey, trailingReplayString(bytes))
        warmed += 1
        break
      } catch {
        // Missing or unreadable snapshot: hydrate falls through to its existing miss path.
      }
    }
  }
  return warmed
}
