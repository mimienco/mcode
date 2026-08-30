import { isTauri } from '@tauri-apps/api/core'
import { prepareRendererForAppRestart } from '../../../shared/renderer-restart-preparation'
import {
  MCODE_APP_RESTART_ABORTED_EVENT,
  MCODE_APP_RESTART_STARTED_EVENT
} from '../../../shared/updater-renderer-events'
import type { DesktopUpdaterCoreCapability } from './tauri-desktop-updater-capability'

type RuntimeDesktopUpdaterDependencies = {
  isTauri: () => boolean
  electron: DesktopUpdaterCoreCapability
  loadTauri: () => Promise<DesktopUpdaterCoreCapability>
  prepareTauriInstall: () => Promise<void>
  abortTauriInstall: () => void
}

export function createRuntimeDesktopUpdaterClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri,
  prepareTauriInstall,
  abortTauriInstall
}: RuntimeDesktopUpdaterDependencies): DesktopUpdaterCoreCapability {
  let tauriClient: Promise<DesktopUpdaterCoreCapability> | undefined
  const resolveClient = (): Promise<DesktopUpdaterCoreCapability> => {
    if (!isTauriRuntime()) {
      return Promise.resolve(electron)
    }
    tauriClient ??= loadTauri()
    return tauriClient
  }

  return {
    getVersion: () =>
      isTauriRuntime()
        ? resolveClient().then((client) => client.getVersion())
        : electron.getVersion(),
    getStatus: () =>
      isTauriRuntime()
        ? resolveClient().then((client) => client.getStatus())
        : electron.getStatus(),
    check: (options) =>
      isTauriRuntime()
        ? resolveClient().then((client) => client.check(options))
        : electron.check(options),
    checkOnLaunch: () =>
      isTauriRuntime()
        ? resolveClient().then((client) => client.checkOnLaunch?.())
        : Promise.resolve(),
    download: () =>
      isTauriRuntime() ? resolveClient().then((client) => client.download()) : electron.download(),
    async quitAndInstall() {
      if (!isTauriRuntime()) {
        return electron.quitAndInstall()
      }
      await prepareTauriInstall()
      try {
        await (await resolveClient()).quitAndInstall()
      } catch (error) {
        abortTauriInstall()
        throw error
      }
    },
    onStatus: (listener) =>
      isTauriRuntime()
        ? resolveClient().then((client) => client.onStatus(listener))
        : electron.onStatus(listener)
  }
}

const electronUpdater: DesktopUpdaterCoreCapability = {
  getVersion: () => window.api.updater.getVersion(),
  getStatus: () => window.api.updater.getStatus(),
  check: (options) => window.api.updater.check(options),
  download: () => window.api.updater.download(),
  quitAndInstall: () => window.api.updater.quitAndInstall(),
  onStatus: (listener) => Promise.resolve(window.api.updater.onStatus(listener))
}

export const desktopUpdaterClient = createRuntimeDesktopUpdaterClient({
  isTauri,
  electron: electronUpdater,
  loadTauri: async () => (await import('./tauri-desktop-client')).desktopUpdater,
  prepareTauriInstall: () =>
    prepareRendererForAppRestart(window, {
      startedEventName: MCODE_APP_RESTART_STARTED_EVENT,
      abortedEventName: MCODE_APP_RESTART_ABORTED_EVENT,
      awaitCheckpoint: () => Promise.resolve()
    }),
  abortTauriInstall: () => {
    window.dispatchEvent(new Event(MCODE_APP_RESTART_ABORTED_EVENT))
  }
})
