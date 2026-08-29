import type { ExecutionHostId } from './execution-host'
import type { PersistedUIState } from './persisted-ui-state-types'
import type { WorkspaceSessionState } from './workspace-session-state-types'

export const SHUTDOWN_CHECKPOINT_VERSION = 1 as const

export type ShutdownCheckpointPayload = {
  sessions: { state: WorkspaceSessionState; hostId?: ExecutionHostId }[]
  ui: Partial<PersistedUIState>
}

export type ShutdownCheckpoint = ShutdownCheckpointPayload & {
  version: typeof SHUTDOWN_CHECKPOINT_VERSION
}

export function versionShutdownCheckpoint(
  checkpoint: ShutdownCheckpointPayload
): ShutdownCheckpoint {
  return {
    version: SHUTDOWN_CHECKPOINT_VERSION,
    ...checkpoint
  }
}
