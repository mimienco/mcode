import type { DesktopWindowCapability, ShutdownRequest } from './desktop-client'

type TauriCloseRequestedEvent = {
  preventDefault: () => void
}

type TauriWindowCapabilitySource = {
  minimize: () => Promise<void>
  toggleMaximize: () => Promise<void>
  isMaximized: () => Promise<boolean>
  onResized: (listener: () => void) => Promise<() => void>
  close: () => Promise<void>
  onCloseRequested: (
    listener: (event: TauriCloseRequestedEvent) => void | Promise<void>
  ) => Promise<() => void>
}

type TauriAppLifecycleCapabilitySource = {
  requestShutdown: (intent: 'quit' | 'restart') => Promise<ShutdownRequest>
  onShutdownRequested: (
    listener: (request: ShutdownRequest) => void | Promise<void>
  ) => Promise<() => void>
  cancelShutdown: (requestId: string) => Promise<void>
  confirmShutdown: (requestId: string) => Promise<ShutdownRequest>
  executeShutdown: (requestId: string) => Promise<void>
}

export function createTauriDesktopWindowCapability(
  currentWindow: TauriWindowCapabilitySource,
  appLifecycle: TauriAppLifecycleCapabilitySource
): DesktopWindowCapability {
  let manualQuitInFlight = false
  let pendingShutdown: ShutdownRequest | undefined
  const clearPendingShutdown = (requestId: string): void => {
    if (pendingShutdown?.requestId === requestId) {
      pendingShutdown = undefined
    }
  }
  const cancelPendingShutdown = async (): Promise<ShutdownRequest | undefined> => {
    if (!pendingShutdown) {
      return undefined
    }
    const request = pendingShutdown
    await appLifecycle.cancelShutdown(request.requestId)
    clearPendingShutdown(request.requestId)
    return request
  }

  return {
    minimize: () => currentWindow.minimize(),
    toggleMaximize: () => currentWindow.toggleMaximize(),
    isMaximized: () => currentWindow.isMaximized(),
    async onMaximizedChanged(listener) {
      let active = true
      let revision = 0
      const unlisten = await currentWindow.onResized(() => {
        const eventRevision = ++revision
        void currentWindow.isMaximized().then((maximized) => {
          if (active && eventRevision === revision) {
            listener(maximized)
          }
        })
      })
      return () => {
        active = false
        unlisten()
      }
    },
    requestClose: () => currentWindow.close(),
    async onCloseRequested(listener) {
      const stopNativeClose = await currentWindow.onCloseRequested(async (event) => {
        event.preventDefault()
        await cancelPendingShutdown()
        await listener({ isQuitting: false })
      })
      try {
        const stopAppShutdown = await appLifecycle.onShutdownRequested(async (request) => {
          pendingShutdown = request
          if (manualQuitInFlight && request.intent === 'quit') {
            return
          }
          await listener({ isQuitting: true })
        })
        return () => {
          stopAppShutdown()
          stopNativeClose()
        }
      } catch (error) {
        stopNativeClose()
        throw error
      }
    },
    cancelClose: cancelPendingShutdown,
    async confirmClose() {
      if (pendingShutdown) {
        const request = pendingShutdown
        await appLifecycle.confirmShutdown(request.requestId)
        await appLifecycle.executeShutdown(request.requestId)
        clearPendingShutdown(request.requestId)
        return
      }
      manualQuitInFlight = true
      try {
        const request = await appLifecycle.requestShutdown('quit')
        await appLifecycle.confirmShutdown(request.requestId)
        await appLifecycle.executeShutdown(request.requestId)
        clearPendingShutdown(request.requestId)
      } finally {
        manualQuitInFlight = false
      }
    }
  }
}
