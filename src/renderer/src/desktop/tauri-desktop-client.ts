import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopClient } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'

const currentWindow = getCurrentWindow()
const desktopInvoke = (command: string, args?: Record<string, unknown>): Promise<unknown> =>
  invoke(command, args)
const appLifecycle = createTauriDesktopAppLifecycleCapability(desktopInvoke, (event, listener) =>
  listen(event, listener)
)

export const desktopClient = createDesktopClient(
  desktopInvoke,
  createTauriDesktopWindowCapability(currentWindow, appLifecycle),
  appLifecycle
)
