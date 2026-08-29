import { isTauri } from '@tauri-apps/api/core'
import type { DesktopClient } from './desktop-client'

type DesktopWindowClient = DesktopClient['window']

type RuntimeDesktopWindowDependencies = {
  isTauri: () => boolean
  electron: DesktopWindowClient
  loadTauri: () => Promise<DesktopWindowClient>
}

export function createRuntimeDesktopWindowClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri
}: RuntimeDesktopWindowDependencies): DesktopWindowClient {
  let tauriClient: Promise<DesktopWindowClient> | undefined
  const resolveClient = (): Promise<DesktopWindowClient> => {
    if (!isTauriRuntime()) {
      return Promise.resolve(electron)
    }
    tauriClient ??= loadTauri()
    return tauriClient
  }

  return {
    minimize: async () => (await resolveClient()).minimize(),
    toggleMaximize: async () => (await resolveClient()).toggleMaximize(),
    isMaximized: async () => (await resolveClient()).isMaximized(),
    onMaximizedChanged: async (listener) => (await resolveClient()).onMaximizedChanged(listener),
    requestClose: async () => (await resolveClient()).requestClose(),
    onCloseRequested: async (listener) => (await resolveClient()).onCloseRequested(listener),
    cancelClose: async () => (await resolveClient()).cancelClose(),
    confirmClose: async () => (await resolveClient()).confirmClose()
  }
}

const electronWindowClient: DesktopWindowClient = {
  minimize: () => {
    window.api.ui.minimize()
    return Promise.resolve()
  },
  toggleMaximize: () => {
    window.api.ui.maximize()
    return Promise.resolve()
  },
  isMaximized: () => window.api.ui.isMaximized(),
  onMaximizedChanged: (listener) => Promise.resolve(window.api.ui.onMaximizeChanged(listener)),
  requestClose: () => {
    window.api.ui.requestClose()
    return Promise.resolve()
  },
  onCloseRequested: (listener) => Promise.resolve(window.api.ui.onWindowCloseRequested(listener)),
  cancelClose: () => Promise.resolve(undefined),
  confirmClose: () => {
    window.api.ui.confirmWindowClose()
    return Promise.resolve()
  }
}

export const desktopWindowClient = createRuntimeDesktopWindowClient({
  isTauri,
  electron: electronWindowClient,
  loadTauri: async () => (await import('./tauri-desktop-client')).desktopClient.window
})
