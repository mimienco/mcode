import { isTauri } from '@tauri-apps/api/core'
import type { PreloadApi } from '../../../preload/api-types'
import { tauriClipboardClient } from './tauri-clipboard-client'
import {
  saveClipboardImageAsTempFile,
  type SaveClipboardImageAsTempFileArgs
} from './tauri-clipboard-image'

/**
 * Partial `ui` bridge over the native Tauri clipboard commands. Keyed by the
 * Electron preload's method names so the 131 renderer callsites of
 * `window.api.ui.readClipboardText` / `writeClipboardText` /
 * `writeTerminalClipboardText` keep working unchanged under Tauri.
 */
const tauriUiClipboardBridge = {
  readClipboardText: (options?: { maxBytes?: number }) =>
    tauriClipboardClient.readClipboardText(options),
  writeClipboardText: (text: string) => tauriClipboardClient.writeClipboardText(text),
  writeTerminalClipboardText: (text: string) =>
    tauriClipboardClient.writeTerminalClipboardText(text),
  saveClipboardImageAsTempFile: (args?: SaveClipboardImageAsTempFileArgs) =>
    saveClipboardImageAsTempFile(args)
}

/**
 * Installs a minimal `window.api` shim before the React tree renders when the
 * renderer boots under Tauri (no Electron preload). The clipboard bridge is
 * merged over any existing `window.api`; under Electron this is a no-op.
 */
export function installTauriWindowApiShim(): void {
  if (!isTauri()) {
    return
  }
  const existing = window.api as Partial<PreloadApi> | undefined
  window.api = {
    ...existing,
    ui: {
      ...existing?.ui,
      ...tauriUiClipboardBridge
    }
  } as unknown as Window['api']
}
