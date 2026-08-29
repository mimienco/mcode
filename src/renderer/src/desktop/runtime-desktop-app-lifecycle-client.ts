import { isTauri } from '@tauri-apps/api/core'
import { prepareRendererForAppRestart } from '../../../shared/renderer-restart-preparation'
import {
  MCODE_APP_RESTART_ABORTED_EVENT,
  MCODE_APP_RESTART_STARTED_EVENT
} from '../../../shared/updater-renderer-events'
import type { DesktopClient } from './desktop-client'

type DesktopRestartApi = Pick<DesktopClient['app'], 'requestShutdown'>

type ElectronRestartApi = {
  restart: () => Promise<void>
  relaunch: () => Promise<void>
}

type RuntimeDesktopAppLifecycleDependencies = {
  isTauri: () => boolean
  electron: ElectronRestartApi
  loadTauri: () => Promise<DesktopRestartApi>
  prepareTauriRestart: () => Promise<void>
  abortTauriRestart: () => void
}

export function createRuntimeDesktopAppLifecycleClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri,
  prepareTauriRestart,
  abortTauriRestart
}: RuntimeDesktopAppLifecycleDependencies): ElectronRestartApi {
  let tauriClient: Promise<DesktopRestartApi> | undefined
  const resolveTauriClient = (): Promise<DesktopRestartApi> => {
    tauriClient ??= loadTauri()
    return tauriClient
  }
  const requestTauriRestart = async (): Promise<void> => {
    await prepareTauriRestart()
    try {
      await (await resolveTauriClient()).requestShutdown('restart')
    } catch (error) {
      abortTauriRestart()
      throw error
    }
  }

  return {
    restart: () => (isTauriRuntime() ? requestTauriRestart() : electron.restart()),
    relaunch: () => (isTauriRuntime() ? requestTauriRestart() : electron.relaunch())
  }
}

const electronAppLifecycle: ElectronRestartApi = {
  restart: () => window.api.app.restart(),
  relaunch: () => window.api.app.relaunch()
}

export const desktopAppLifecycleClient = createRuntimeDesktopAppLifecycleClient({
  isTauri,
  electron: electronAppLifecycle,
  loadTauri: async () => (await import('./tauri-desktop-client')).desktopClient.app,
  prepareTauriRestart: () =>
    prepareRendererForAppRestart(window, {
      startedEventName: MCODE_APP_RESTART_STARTED_EVENT,
      abortedEventName: MCODE_APP_RESTART_ABORTED_EVENT,
      awaitCheckpoint: () => Promise.resolve()
    }),
  abortTauriRestart: () => {
    window.dispatchEvent(new Event(MCODE_APP_RESTART_ABORTED_EVENT))
  }
})
