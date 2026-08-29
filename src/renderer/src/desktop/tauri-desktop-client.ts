import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopClient } from './desktop-client'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'

const currentWindow = getCurrentWindow()

export const desktopClient = createDesktopClient(
  (command, args) => invoke(command, args),
  createTauriDesktopWindowCapability(currentWindow)
)
