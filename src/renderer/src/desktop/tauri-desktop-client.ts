import { invoke } from '@tauri-apps/api/core'
import { createDesktopClient } from './desktop-client'

export const desktopClient = createDesktopClient((command, args) => invoke(command, args))
