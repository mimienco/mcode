import type { DesktopWindowCapability } from './desktop-client'

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

export function createTauriDesktopWindowCapability(
  currentWindow: TauriWindowCapabilitySource
): DesktopWindowCapability {
  let closeConfirmed = false

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
    onCloseRequested: (listener) =>
      currentWindow.onCloseRequested((event) => {
        if (closeConfirmed) {
          closeConfirmed = false
          return
        }
        event.preventDefault()
        listener({ isQuitting: false })
      }),
    async confirmClose() {
      closeConfirmed = true
      try {
        await currentWindow.close()
      } catch (error) {
        closeConfirmed = false
        throw error
      }
    }
  }
}
