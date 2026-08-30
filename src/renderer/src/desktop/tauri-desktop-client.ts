import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopClient, createDesktopInvoke } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'
import { createTauriDesktopUpdaterCapability } from './tauri-desktop-updater-capability'
import { MCODE_APP_RESTART_ABORTED_EVENT } from '../../../shared/updater-renderer-events'

const currentWindow = getCurrentWindow()
const desktopInvoke = createDesktopInvoke((command, args) => invoke(command, args))
const appLifecycle = createTauriDesktopAppLifecycleCapability(desktopInvoke, (event, listener) =>
  listen(event, listener)
)

export const tauriCloseToTrayClient = {
  async setEnabled(enabled: boolean): Promise<void> {
    await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled })
  }
}

export const desktopClient = createDesktopClient(
  desktopInvoke,
  createTauriDesktopWindowCapability(currentWindow, appLifecycle, async () => {
    const hidden = await desktopInvoke('window_hide_to_tray_if_enabled')
    if (typeof hidden !== 'boolean') {
      throw new Error('window_hide_to_tray_if_enabled returned a non-boolean value')
    }
    return hidden
  }),
  appLifecycle
)

export const desktopUpdater = createTauriDesktopUpdaterCapability(
  desktopInvoke,
  (event, listener) => listen(event, listener),
  async () => (await desktopClient.app.getLaunchContext()).version,
  () => window.dispatchEvent(new Event(MCODE_APP_RESTART_ABORTED_EVENT))
)
