import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopClient } from './desktop-client'

const currentWindow = getCurrentWindow()

export const desktopClient = createDesktopClient((command, args) => invoke(command, args), {
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
  requestClose: () => currentWindow.close()
})
