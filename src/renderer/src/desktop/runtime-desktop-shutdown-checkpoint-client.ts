import { isTauri } from '@tauri-apps/api/core'
import {
  versionShutdownCheckpoint,
  type ShutdownCheckpointPayload
} from '../../../shared/shutdown-checkpoint-types'
import type { DesktopClient } from './desktop-client'

type TauriShutdownCheckpointClient = Pick<DesktopClient['app'], 'stageShutdownCheckpoint'>

type ElectronShutdownCheckpointClient = {
  stageSync: (checkpoint: ShutdownCheckpointPayload) => void
  awaitDurable: () => Promise<void>
}

type RuntimeDesktopShutdownCheckpointDependencies = {
  isTauri: () => boolean
  electron: ElectronShutdownCheckpointClient
  loadTauri: () => Promise<TauriShutdownCheckpointClient>
}

export type RuntimeDesktopShutdownCheckpointClient = {
  strategy: 'sync-beforeunload' | 'async-close-guard'
  stageSync: (checkpoint: ShutdownCheckpointPayload) => void
  stageAndWait: (checkpoint: ShutdownCheckpointPayload) => Promise<void>
}

export function createRuntimeDesktopShutdownCheckpointClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri
}: RuntimeDesktopShutdownCheckpointDependencies): RuntimeDesktopShutdownCheckpointClient {
  const tauriRuntime = isTauriRuntime()
  let tauriClient: Promise<TauriShutdownCheckpointClient> | undefined
  const resolveTauriClient = (): Promise<TauriShutdownCheckpointClient> => {
    tauriClient ??= loadTauri()
    return tauriClient
  }

  return {
    strategy: tauriRuntime ? 'async-close-guard' : 'sync-beforeunload',
    stageSync(checkpoint): void {
      if (tauriRuntime) {
        throw new Error('Tauri shutdown checkpoints cannot stage synchronously')
      }
      electron.stageSync(checkpoint)
    },
    async stageAndWait(checkpoint): Promise<void> {
      if (tauriRuntime) {
        const client = await resolveTauriClient()
        await client.stageShutdownCheckpoint(versionShutdownCheckpoint(checkpoint))
        return
      }
      electron.stageSync(checkpoint)
      await electron.awaitDurable()
    }
  }
}

export const desktopShutdownCheckpointClient = createRuntimeDesktopShutdownCheckpointClient({
  isTauri,
  electron: {
    stageSync: (checkpoint) => window.api.app.stageBeforeUnloadSync(checkpoint),
    awaitDurable: () => window.api.app.awaitBeforeUnloadCheckpoint()
  },
  loadTauri: async () => (await import('./tauri-desktop-client')).desktopClient.app
})
