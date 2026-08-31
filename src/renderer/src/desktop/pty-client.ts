import { createDesktopInvoke } from './desktop-client'

// ─── Wire DTOs (mirror the Rust serde shapes in src-tauri/pty_commands.rs) ──

export type PtySpawnRequest = {
  program?: string
  argVec?: string[]
  cwd?: string
  shellOverride?: string
  cols: number
  rows: number
  env?: Record<string, string>
}

export type PtySpawnResponse = {
  id: number
  shell: string
}

export type PtyDataChunk = {
  data: string
  /** Cumulative output sequence AFTER this chunk (char count). */
  seq: number
  rawLength?: number
  transformed?: boolean
}

export type PtyRestoreMarker = {
  reason: 'hidden_drop' | 'unhide' | 'pending_cap' | 'delivery_heal'
  markerSeq: number
}

export type PtyDataPayload = {
  id: number
  chunks: PtyDataChunk[]
  dropped?: boolean
  restoreMarker?: PtyRestoreMarker
}

export type PtyExitPayload = {
  id: number
  code: number
}

export type PtySubscription = () => void

// ─── Typed API surface (one method per pty_* command) ───────────────────────

export type PtyApi = {
  spawn: (spec: PtySpawnRequest) => Promise<PtySpawnResponse>
  write: (id: number, data: string) => Promise<void>
  resize: (id: number, cols: number, rows: number) => Promise<void>
  kill: (id: number) => Promise<void>
  hasPty: (id: number) => Promise<boolean>
  onData: (listener: (payload: PtyDataPayload) => void) => Promise<PtySubscription>
  onExit: (listener: (payload: PtyExitPayload) => void) => Promise<PtySubscription>
}

type PtyInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

type PtyEventSource = (
  event: string,
  listener: (payload: { payload: unknown }) => void
) => Promise<() => void>

export const PTY_DATA_EVENT = 'pty://data'
export const PTY_EXIT_EVENT = 'pty://exit'

/**
 * The native pty commands serialize `Option` error-context fields as JSON
 * `null`, but the shared `DesktopCommandError` guard treats only `undefined`
 * as absent. Nulls are stripped so the payload keeps its native code.
 */
function stripNullContextFields(error: unknown): unknown {
  if (
    !error ||
    typeof error !== 'object' ||
    !('context' in error) ||
    !error.context ||
    typeof error.context !== 'object' ||
    Array.isArray(error.context)
  ) {
    return error
  }
  const { context, ...rest } = error
  const cleaned = Object.fromEntries(Object.entries(context).filter(([, value]) => value !== null))
  return { ...rest, context: cleaned }
}

/**
 * Builds the typed pty API over any invoke transport. Raw command strings
 * live only here; callers receive one typed method per `pty_*` command and
 * every failure is normalized onto `DesktopCommandError`.
 */
export function createTauriPtyClient(invoke: PtyInvoke, listen: PtyEventSource): PtyApi {
  const desktopInvoke = createDesktopInvoke((command, args) =>
    invoke(command, args).catch((error: unknown) => {
      throw stripNullContextFields(error)
    })
  )
  const call = (command: string, args?: Record<string, unknown>): Promise<unknown> =>
    desktopInvoke(command, args)

  return {
    spawn: (spec) => call('pty_spawn', { spec }) as Promise<PtySpawnResponse>,
    write: async (id, data) => {
      await call('pty_write', { id, data })
    },
    resize: async (id, cols, rows) => {
      await call('pty_resize', { id, cols, rows })
    },
    kill: async (id) => {
      await call('pty_kill', { id })
    },
    hasPty: (id) => call('pty_has_pty', { id }) as Promise<boolean>,
    async onData(listener) {
      const unlisten = await listen(PTY_DATA_EVENT, (event) => {
        const payload = event.payload as PtyDataPayload | null
        if (
          payload &&
          typeof payload === 'object' &&
          typeof payload.id === 'number' &&
          Array.isArray(payload.chunks)
        ) {
          listener(payload)
        }
      })
      return unlisten
    },
    async onExit(listener) {
      const unlisten = await listen(PTY_EXIT_EVENT, (event) => {
        const payload = event.payload as PtyExitPayload | null
        if (
          payload &&
          typeof payload === 'object' &&
          typeof payload.id === 'number' &&
          typeof payload.code === 'number'
        ) {
          listener(payload)
        }
      })
      return unlisten
    }
  }
}
