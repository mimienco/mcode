import { createDesktopInvoke } from './desktop-client'

// ─── Wire DTOs (mirror the Rust serde shapes in src-tauri/src/ssh_commands.rs) ──

export type SshAuthDto =
  | { type: 'key'; keyPath: string; passphrase?: string }
  | { type: 'password'; password: string }
  | { type: 'agent' }

export type SshTargetDto = {
  host: string
  port: number
  username: string
  auth: SshAuthDto
  connectTimeoutMs?: number
}

export type SshConnectResponse =
  | { status: 'connected'; connectionId: number }
  | {
      status: 'host-key-unknown'
      connectAttemptId: number
      fingerprint: string
      algorithm: string
      keyBase64: string
    }
  | { status: 'host-key-changed'; fingerprint: string; algorithm: string }
  | { status: 'rejected' }

export type SshExecResponse = {
  stdout: string
  stderr: string
  exitCode: number
}

export type SshOpenPtyResponse = {
  ptyId: string
}

// ─── Typed API surface (one method per ssh_* command) ───────────────────────

export type SshApi = {
  connect: (target: SshTargetDto) => Promise<SshConnectResponse>
  acceptHostKey: (connectAttemptId: number, accept: boolean) => Promise<SshConnectResponse>
  exec: (connectionId: number, command: string, timeoutMs?: number) => Promise<SshExecResponse>
  openPty: (connectionId: number, cols: number, rows: number) => Promise<SshOpenPtyResponse>
  writePty: (ptyId: string, data: string) => Promise<void>
  resizePty: (ptyId: string, cols: number, rows: number) => Promise<void>
  closePty: (ptyId: string) => Promise<void>
  disconnect: (connectionId: number) => Promise<void>
}

type SshInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

/**
 * The native ssh commands serialize `Option` error-context fields as JSON
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

const SSH_EXEC_DEFAULT_TIMEOUT_MS = 120_000
const SSH_EXEC_MIN_TIMEOUT_MS = 1_000
const SSH_EXEC_MAX_TIMEOUT_MS = 600_000

/**
 * Builds the typed ssh API over any invoke transport. Raw command strings
 * live only here; callers receive one typed method per `ssh_*` command and
 * every failure is normalized onto `DesktopCommandError`.
 */
export function createTauriSshClient(invoke: SshInvoke): SshApi {
  const desktopInvoke = createDesktopInvoke((command, args) =>
    invoke(command, args).catch((error: unknown) => {
      throw stripNullContextFields(error)
    })
  )
  const call = (command: string, args?: Record<string, unknown>): Promise<unknown> =>
    desktopInvoke(command, args)

  return {
    connect: (target) => call('ssh_connect', { target }) as Promise<SshConnectResponse>,
    acceptHostKey: (connectAttemptId, accept) =>
      call('ssh_accept_host_key', { connectAttemptId, accept }) as Promise<SshConnectResponse>,
    exec: (connectionId, command, timeoutMs) => {
      const clampedTimeoutMs =
        timeoutMs === undefined
          ? SSH_EXEC_DEFAULT_TIMEOUT_MS
          : Math.min(
              SSH_EXEC_MAX_TIMEOUT_MS,
              Math.max(SSH_EXEC_MIN_TIMEOUT_MS, Math.floor(timeoutMs))
            )
      return call('ssh_exec', {
        connectionId,
        command,
        timeoutMs: clampedTimeoutMs
      }) as Promise<SshExecResponse>
    },
    openPty: (connectionId, cols, rows) =>
      call('ssh_open_pty', { connectionId, cols, rows }) as Promise<SshOpenPtyResponse>,
    writePty: async (ptyId, data) => {
      await call('ssh_write_pty', { ptyId, data })
    },
    resizePty: async (ptyId, cols, rows) => {
      await call('ssh_resize_pty', { ptyId, cols, rows })
    },
    closePty: async (ptyId) => {
      await call('ssh_close_pty', { ptyId })
    },
    disconnect: async (connectionId) => {
      await call('ssh_disconnect', { connectionId })
    }
  }
}
