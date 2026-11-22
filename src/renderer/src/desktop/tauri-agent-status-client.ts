import { createDesktopInvoke, DesktopCommandError } from './desktop-client'
import type { AgentStatusApi } from '../../../preload/api/agent-status-api'
import type {
  AgentStatusClearIpcPayload,
  AgentStatusIpcPayload,
  AgentStatusState,
  MigrationUnsupportedPtyEntry
} from '../../../shared/agent-status-types'
import { AGENT_STATUS_STATES } from '../../../shared/agent-status-types'

export const AGENT_STATUS_SET_EVENT = 'agentStatus:set'
export const AGENT_STATUS_CLEAR_EVENT = 'agentStatus:clear'
export const AGENT_STATUS_MIGRATION_UNSUPPORTED_EVENT = 'agentStatus:migrationUnsupported'
export const AGENT_STATUS_MIGRATION_UNSUPPORTED_CLEAR_EVENT = 'agentStatus:migrationUnsupportedClear'
export const AGENT_STATUS_LEGACY_WORKER_TERMINAL_RECOVERY_EVENT =
  'agentStatus:legacyWorkerTerminalRecovery'

export type AgentStatusSubscription = () => void

export type AgentStatusLegacyWorkerTerminalRecovery = {
  paneKey: string
  resolution: 'adopted' | 'exited' | 'rolled_back'
  ptyId?: string
}

type AgentStatusInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

type AgentStatusEventSource = (
  event: string,
  listener: (event: { payload: unknown }) => void
) => Promise<() => void>

const VALID_STATES: ReadonlySet<string> = new Set<string>(AGENT_STATUS_STATES)
const LEGACY_RESOLUTIONS: ReadonlySet<string> = new Set(['adopted', 'exited', 'rolled_back'])

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

function isStatusState(value: unknown): value is AgentStatusState {
  return typeof value === 'string' && VALID_STATES.has(value)
}

export function asAgentStatusIpcPayload(value: unknown): AgentStatusIpcPayload | undefined {
  if (!isRecord(value) || !isStatusState(value.state) || typeof value.prompt !== 'string') {
    return undefined
  }
  if (typeof value.paneKey !== 'string' || value.paneKey.length === 0) {
    return undefined
  }
  if (value.connectionId !== null && typeof value.connectionId !== 'string') {
    return undefined
  }
  if (!isFiniteNumber(value.receivedAt) || !isFiniteNumber(value.stateStartedAt)) {
    return undefined
  }
  return value as unknown as AgentStatusIpcPayload
}

export function asAgentStatusClearIpcPayload(
  value: unknown
): AgentStatusClearIpcPayload | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  if (value.transient === true) {
    if (typeof value.connectionId !== 'string' || value.connectionId.length === 0) {
      return undefined
    }
    if (!isFiniteNumber(value.clearedAt)) {
      return undefined
    }
    return {
      transient: true,
      connectionId: value.connectionId,
      clearedAt: value.clearedAt
    }
  }
  if (typeof value.paneKey !== 'string' || value.paneKey.length === 0) {
    return undefined
  }
  return { paneKey: value.paneKey }
}

export function asMigrationUnsupportedPtyEntry(
  value: unknown
): MigrationUnsupportedPtyEntry | undefined {
  if (!isRecord(value) || typeof value.ptyId !== 'string' || value.ptyId.length === 0) {
    return undefined
  }
  if (value.reason !== 'legacy-numeric-pane-key') {
    return undefined
  }
  if (value.source !== 'local' && value.source !== 'ssh') {
    return undefined
  }
  if (!isFiniteNumber(value.updatedAt)) {
    return undefined
  }
  return value as unknown as MigrationUnsupportedPtyEntry
}

function asMigrationUnsupportedClear(value: unknown): { ptyId: string } | undefined {
  if (!isRecord(value) || typeof value.ptyId !== 'string' || value.ptyId.length === 0) {
    return undefined
  }
  return { ptyId: value.ptyId }
}

export function asLegacyWorkerTerminalRecovery(
  value: unknown
): AgentStatusLegacyWorkerTerminalRecovery | undefined {
  if (!isRecord(value) || typeof value.paneKey !== 'string' || value.paneKey.length === 0) {
    return undefined
  }
  if (typeof value.resolution !== 'string' || !LEGACY_RESOLUTIONS.has(value.resolution)) {
    return undefined
  }
  if (value.ptyId !== undefined && typeof value.ptyId !== 'string') {
    return undefined
  }
  return value as AgentStatusLegacyWorkerTerminalRecovery
}

function invalidResponse(operation: string, value: unknown): DesktopCommandError {
  return new DesktopCommandError(
    `${operation}_invalid_response`,
    { operation },
    `${operation} returned an unexpected payload`,
    'MCode could not complete the desktop request. Try again.',
    value
  )
}

function asBoolean(operation: string, value: unknown): boolean {
  if (typeof value !== 'boolean') {
    throw invalidResponse(operation, value)
  }
  return value
}

/**
 * Typed `window.api.agentStatus` over Tauri invoke/listen. Tauri `listen()` is
 * async; preload returns a synchronous unlisten, so arming is deferred.
 */
export function createTauriAgentStatusClient(
  invoke: AgentStatusInvoke,
  listen: AgentStatusEventSource
): AgentStatusApi {
  const call = createDesktopInvoke(invoke)

  function subscribe<T>(
    event: string,
    guard: (payload: unknown) => T | undefined,
    callback: (value: T) => void
  ): AgentStatusSubscription {
    let unlisten: (() => void) | undefined
    let disposed = false
    void listen(event, (e) => {
      if (disposed) {
        return
      }
      const value = guard(e.payload)
      if (value !== undefined) {
        callback(value)
      }
    }).then(
      (off) => {
        if (disposed) {
          off()
        } else {
          unlisten = off
        }
      },
      () => undefined
    )
    return () => {
      disposed = true
      unlisten?.()
    }
  }

  return {
    onSet: (callback) => subscribe(AGENT_STATUS_SET_EVENT, asAgentStatusIpcPayload, callback),
    onClear: (callback) =>
      subscribe(AGENT_STATUS_CLEAR_EVENT, asAgentStatusClearIpcPayload, callback),
    onMigrationUnsupported: (callback) =>
      subscribe(
        AGENT_STATUS_MIGRATION_UNSUPPORTED_EVENT,
        asMigrationUnsupportedPtyEntry,
        callback
      ),
    onMigrationUnsupportedClear: (callback) =>
      subscribe(
        AGENT_STATUS_MIGRATION_UNSUPPORTED_CLEAR_EVENT,
        asMigrationUnsupportedClear,
        callback
      ),
    onLegacyWorkerTerminalRecovery: (callback) =>
      subscribe(
        AGENT_STATUS_LEGACY_WORKER_TERMINAL_RECOVERY_EVENT,
        asLegacyWorkerTerminalRecovery,
        callback
      ),
    getSnapshot: async () => {
      const payload = await call('agent_status_get_snapshot')
      if (!Array.isArray(payload)) {
        throw invalidResponse('agent_status_get_snapshot', payload)
      }
      return payload.flatMap((entry) => {
        const parsed = asAgentStatusIpcPayload(entry)
        return parsed ? [parsed] : []
      })
    },
    inferInterrupt: async (request) =>
      asBoolean(
        'agent_status_infer_interrupt',
        await call('agent_status_infer_interrupt', { request })
      ),
    inferQuestionAnswered: async (request) =>
      asBoolean(
        'agent_status_infer_question_answered',
        await call('agent_status_infer_question_answered', { request })
      ),
    getMigrationUnsupportedSnapshot: async () => {
      const payload = await call('agent_status_get_migration_unsupported_snapshot')
      if (!Array.isArray(payload)) {
        throw invalidResponse('agent_status_get_migration_unsupported_snapshot', payload)
      }
      return payload.flatMap((entry) => {
        const parsed = asMigrationUnsupportedPtyEntry(entry)
        return parsed ? [parsed] : []
      })
    },
    drop: (paneKey) => {
      void call('agent_status_drop', { paneKey }).catch(() => undefined)
    },
    dropByTabPrefix: (tabId) => {
      void call('agent_status_drop_by_tab_prefix', { tabId }).catch(() => undefined)
    },
    retirePaneAuthority: (paneKey) => {
      void call('agent_status_retire_pane_authority', { paneKey }).catch(() => undefined)
    },
    restorePaneAuthority: (paneKey) => {
      void call('agent_status_restore_pane_authority', { paneKey }).catch(() => undefined)
    },
    transferPaneAuthority: (args) => {
      void call('agent_status_transfer_pane_authority', {
        fromPaneKey: args.fromPaneKey,
        toPaneKey: args.toPaneKey,
        ...(args.ptyId !== undefined ? { ptyId: args.ptyId } : {})
      }).catch(() => undefined)
    }
  }
}
