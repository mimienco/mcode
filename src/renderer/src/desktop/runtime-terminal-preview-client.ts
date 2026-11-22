import { isTauri } from '@tauri-apps/api/core'
import {
  tauriTerminalPreviewClient,
  type TauriTerminalPreviewClient
} from './tauri-terminal-preview-client'

type Dependencies = {
  isTauri: () => boolean
  electron: () => TauriTerminalPreviewClient
  tauri: TauriTerminalPreviewClient
}

export function createRuntimeTerminalPreviewClient({
  isTauri: isTauriRuntime,
  electron,
  tauri
}: Dependencies): TauriTerminalPreviewClient {
  const client = (): TauriTerminalPreviewClient => (isTauriRuntime() ? tauri : electron())
  return {
    connect: (ptyId, options) => client().connect(ptyId, options),
    input: (ptyId, data) => client().input(ptyId, data),
    fit: (ptyId, cols, rows) => client().fit(ptyId, cols, rows),
    ack: (ptyId, bytes) => client().ack(ptyId, bytes),
    unsubscribe: (ptyId) => client().unsubscribe(ptyId),
    onData: (callback) => client().onData(callback)
  }
}

export const runtimeTerminalPreviewClient = createRuntimeTerminalPreviewClient({
  isTauri,
  electron: () => window.api.terminalPreview,
  tauri: tauriTerminalPreviewClient
})
