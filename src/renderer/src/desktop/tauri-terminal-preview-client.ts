import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  TerminalPreviewConnectResult,
  TerminalPreviewDataPayload
} from '../../../shared/terminal-preview'

/**
 * Raw DTOs served by the Rust terminal-preview commands
 * (`src-tauri/src/terminal_preview.rs`, serde camelCase). The client below
 * carries the Electron `window.api.terminalPreview` shape directly, so the
 * shim merge (`tauri-window-api-shim.ts`) spreads it verbatim and the
 * dashboard-popout consumers keep calling the preload surface unchanged.
 */

/** Wire DTO of the Rust `TerminalPreviewChunk`. */
export type TerminalPreviewChunkDto = {
  /** Monotonic per-connection sequence; the renderer ACKs the highest processed seq. */
  seq: number
  /** Incremental output; null on the connect echo (which carries only `snapshot`). */
  chunk: string | null
  /** Full replacement payload on cold connect / ring replay; null on live chunks. */
  snapshot: string | null
}

/** Pinned to `TERMINAL_PREVIEW_DATA_EVENT` in `src-tauri/src/terminal_preview.rs`. */
export const TERMINAL_PREVIEW_DATA_EVENT = 'terminal-preview://data'

// Why 80/24: the Rust connect payload carries no terminal grid (the ring
// replay is plain text), so the adapter synthesizes the same fallback the
// consumer applies to missing cols/rows; the preview's fit claim immediately
// resizes the PTY to the dialog's real grid.
const SNAPSHOT_FALLBACK_COLS = 80
const SNAPSHOT_FALLBACK_ROWS = 24

export type TerminalPreviewInvoke = (
  command: string,
  args?: Record<string, unknown>
) => Promise<unknown>

export type TerminalPreviewListen = <T>(
  event: string,
  handler: (event: { payload: T }) => void
) => Promise<() => void>

/**
 * Electron `window.api.terminalPreview` surface
 * (src/preload/api/dashboard-api.ts) served over the Tauri commands.
 */
export type TauriTerminalPreviewClient = {
  connect: (
    ptyId: string,
    opts?: { scrollbackRows?: number }
  ) => Promise<TerminalPreviewConnectResult>
  input: (ptyId: string, data: string) => Promise<boolean>
  /** Claim the PTY grid for the preview dialog; resolves to the size actually in effect. */
  fit: (ptyId: string, cols: number, rows: number) => Promise<{ cols: number; rows: number } | null>
  ack: (ptyId: string, bytes: number) => Promise<void>
  unsubscribe: (ptyId: string) => Promise<void>
  onData: (callback: (payload: TerminalPreviewDataPayload) => void) => () => void
}

function asChunkDto(value: unknown): TerminalPreviewChunkDto | null {
  if (typeof value !== 'object' || value === null) {
    return null
  }
  const dto = value as Partial<TerminalPreviewChunkDto>
  if (typeof dto.seq !== 'number') {
    return null
  }
  return {
    seq: dto.seq,
    chunk: typeof dto.chunk === 'string' ? dto.chunk : null,
    snapshot: typeof dto.snapshot === 'string' ? dto.snapshot : null
  }
}

/** Maps the cold-connect payload; a null/empty snapshot means the pty is gone. */
function toConnectResult(payload: unknown): TerminalPreviewConnectResult {
  const dto = asChunkDto(payload)
  if (!dto || dto.snapshot === null || dto.snapshot.length === 0) {
    return { snapshot: null, replay: [] }
  }
  return {
    snapshot: {
      data: dto.snapshot,
      cols: SNAPSHOT_FALLBACK_COLS,
      rows: SNAPSHOT_FALLBACK_ROWS,
      seq: dto.seq
    },
    replay: []
  }
}

/**
 * The Rust event carries `(ptyId, TerminalPreviewChunk)` tuples; only live
 * chunks (non-null `chunk`) become Electron data payloads — the connect echo
 * and snapshot replays carry only `snapshot` and are not live data.
 *
 * Why the ACK cursor rides in `bytes`: the Electron payload's `bytes` field is
 * transport-private — the renderer treats it as opaque and echoes it back into
 * `ack()`. The Rust transport raises its in-flight window per emitted chunk
 * and expects the highest processed `seq` back, so the adapter publishes the
 * chunk's seq as the cursor (see the `src-tauri/src/terminal_preview.rs`
 * module doc).
 */
function toDataPayload(payload: unknown): TerminalPreviewDataPayload | null {
  if (!Array.isArray(payload) || payload.length !== 2) {
    return null
  }
  const [ptyId, chunk] = payload as unknown[]
  const dto = asChunkDto(chunk)
  if (typeof ptyId !== 'string' || !dto || dto.chunk === null) {
    return null
  }
  return { type: 'data', ptyId, data: dto.chunk, bytes: dto.seq }
}

/**
 * Why: Tauri `listen` resolves asynchronously while the Electron contract
 * returns a synchronous unlisten. Gate delivery on an unsubscribe flag so
 * events are dropped from the instant the caller unsubscribes, and detach the
 * underlying listener whether the unsubscribe lands before or after the
 * subscription settles (same shape as the speech client's `subscribe`).
 */
function subscribeData(
  listen: TerminalPreviewListen,
  callback: (payload: TerminalPreviewDataPayload) => void
): () => void {
  let unsubscribed = false
  let detach: (() => void) | null = null
  const listening = listen<unknown>(TERMINAL_PREVIEW_DATA_EVENT, (event) => {
    const payload = toDataPayload(event.payload)
    if (payload && !unsubscribed) {
      callback(payload)
    }
  })
  void listening.then(
    (unlisten) => {
      detach = unlisten
      if (unsubscribed) {
        unlisten()
        detach = null
      }
    },
    () => undefined
  )
  return () => {
    unsubscribed = true
    detach?.()
    detach = null
  }
}

/**
 * Tauri invoke-backed terminal-preview capability. Command names and the
 * event channel are pinned to `src-tauri/src/terminal_preview.rs`
 * (`rename_as` + `app.emit`). Every method mirrors the Electron bridge's
 * error contract: connect resolves to the pty-gone result instead of
 * rejecting, input/fit report `false`/`null` on failure, and the
 * fire-and-forget ack/unsubscribe never reject.
 */
export function createTauriTerminalPreviewClient(
  invoke: TerminalPreviewInvoke,
  listen: TerminalPreviewListen
): TauriTerminalPreviewClient {
  return {
    // Why the Electron `opts` argument is dropped: scrollbackRows tuned the
    // buffer serialization in main; the Rust v1 ring replay has no such knob
    // (a cold connect always replays the full ring).
    async connect(ptyId) {
      try {
        return toConnectResult(await invoke('terminalPreviewConnect', { ptyId }))
      } catch {
        return { snapshot: null, replay: [] }
      }
    },
    async input(ptyId, data) {
      try {
        await invoke('terminalPreviewInput', { ptyId, data })
        return true
      } catch {
        return false
      }
    },
    async fit(ptyId, cols, rows) {
      try {
        await invoke('terminalPreviewFit', { ptyId, cols, rows })
        // Coalesced resize: the PTY applies the latest requested size, so the
        // requested grid is the size in effect.
        return { cols, rows }
      } catch {
        return null
      }
    },
    async ack(ptyId, bytes) {
      try {
        await invoke('terminalPreviewAck', { ptyId, seq: bytes })
      } catch {
        // ACK after release is a silent no-op in the Electron bridge too.
      }
    },
    async unsubscribe(ptyId) {
      try {
        await invoke('terminalPreviewUnsubscribe', { ptyId })
      } catch {
        // Never reject: consumers unsubscribe fire-and-forget on dispose.
      }
    },
    onData: (callback) => subscribeData(listen, callback)
  }
}

export const tauriTerminalPreviewClient = createTauriTerminalPreviewClient(
  (command, args) => invoke(command, args),
  listen
)
