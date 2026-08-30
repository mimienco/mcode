import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopInvoke, DesktopCommandError, type ShutdownIntent } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { runLifecycleSmoke } from './lifecycle-smoke-client'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'
import { runtimeDesktopFilesystemClient } from './runtime-desktop-filesystem-client'

const smokeParameters = new URLSearchParams(window.location.search)
const intent: ShutdownIntent = smokeParameters.get('intent') === 'restart' ? 'restart' : 'quit'
const shouldCheckUpdater = smokeParameters.get('updater') === 'true'
const shouldCheckWindowing = smokeParameters.get('windowing') === 'true'
const shouldCheckFilesystem = smokeParameters.get('filesystem') === 'true'
const desktopInvoke = createDesktopInvoke((command, args) => invoke(command, args))
const lifecycle = createTauriDesktopAppLifecycleCapability(desktopInvoke, (event, listener) =>
  listen(event, listener)
)

void runLifecycleSmoke({
  lifecycle,
  intent,
  stageCheckpoint: (checkpoint) =>
    desktopInvoke('app_stage_shutdown_checkpoint', { checkpoint }).then(() => undefined)
})
  .then(async () => {
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
    if (shouldCheckFilesystem) {
      const root = smokeParameters.get('root')
      if (!root) {
        throw new Error('filesystem smoke root is missing')
      }
      const separator = root.includes('\\') ? '\\' : '/'
      const childPath = `${root}${separator}visible.txt`
      const missingPath = `${root}${separator}missing.txt`
      const entries = await runtimeDesktopFilesystemClient.readDir({ dirPath: root })
      const metadata = await runtimeDesktopFilesystemClient.stat({ filePath: childPath })
      const missingExists = await runtimeDesktopFilesystemClient.pathExists({
        filePath: missingPath
      })
      let outsideDenied = false
      try {
        await runtimeDesktopFilesystemClient.pathExists({ filePath: `${root}-outside` })
      } catch (error) {
        outsideDenied = error instanceof DesktopCommandError && error.code === 'fs_path_denied'
      }
      if (
        !entries.some((entry) => entry.name === 'visible.txt' && !entry.isDirectory) ||
        metadata.size !== 7 ||
        metadata.isDirectory ||
        missingExists ||
        !outsideDenied
      ) {
        throw new Error('filesystem smoke assertions failed')
      }
      await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true })
      document.getElementById('status')?.replaceChildren('Filesystem smoke passed')
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
  })
  .catch((error: unknown) => {
    console.error('[smoke] lifecycle smoke failed', error)
  })
