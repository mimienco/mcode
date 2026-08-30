import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopInvoke, type ShutdownIntent } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { runLifecycleSmoke } from './lifecycle-smoke-client'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'

const smokeParameters = new URLSearchParams(window.location.search)
const intent: ShutdownIntent = smokeParameters.get('intent') === 'restart' ? 'restart' : 'quit'
const shouldCheckUpdater = smokeParameters.get('updater') === 'true'
const shouldCheckWindowing = smokeParameters.get('windowing') === 'true'
const desktopInvoke = createDesktopInvoke((command, args) => invoke(command, args))
const lifecycle = createTauriDesktopAppLifecycleCapability(desktopInvoke, (event, listener) =>
  listen(event, listener)
)

void runLifecycleSmoke({
  lifecycle,
  intent,
  stageCheckpoint: (checkpoint) =>
    desktopInvoke('app_stage_shutdown_checkpoint', { checkpoint }).then(() => undefined)
}).then(
  async () => {
    document.getElementById('status')?.replaceChildren('Lifecycle listener ready')
    if (shouldCheckWindowing) {
      const currentWindow = getCurrentWindow()
      const windowCapability = createTauriDesktopWindowCapability(
        currentWindow,
        lifecycle,
        async () => {
          const hidden = await desktopInvoke('window_hide_to_tray_if_enabled')
          if (typeof hidden !== 'boolean') {
            throw new Error('window_hide_to_tray_if_enabled returned a non-boolean value')
          }
          return hidden
        }
      )
      const stopWindowingSmoke = await windowCapability.onCloseRequested(() => {
        throw new Error('windowing smoke unexpectedly entered the dirty-close coordinator')
      })
      // Preference synchronization is deliberately last: its native revision is the
      // smoke driver's readiness handshake after both event listeners are installed.
      await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true })
      let hidden = false
      for (let attempt = 0; attempt < 30; attempt += 1) {
        if (!(await currentWindow.isVisible())) {
          hidden = true
          break
        }
        await new Promise((resolve) => window.setTimeout(resolve, 50))
      }
      stopWindowingSmoke()
      if (!hidden) {
        throw new Error('native close did not hide the window after renderer readiness')
      }
      return
    }
    if (!shouldCheckUpdater) {
      return
    }
    try {
      await desktopInvoke('update_check', {
        request: { channel: 'stable', userInitiated: true }
      })
      throw new Error('unconfigured updater smoke unexpectedly succeeded')
    } catch (error) {
      const code =
        error && typeof error === 'object' && 'code' in error ? String(error.code) : 'unknown_error'
      document.getElementById('status')?.replaceChildren(`Updater smoke result: ${code}`)
    }
  },
  (error) => {
    console.error('[smoke] lifecycle smoke failed', error)
  }
)
