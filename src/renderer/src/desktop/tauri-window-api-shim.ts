import { isTauri, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { PreloadApi } from '../../../preload/api-types'
import { tauriSpeechBridge } from './tauri-speech-shim'
import { tauriClipboardClient } from './tauri-clipboard-client'
import {
  saveClipboardImageAsTempFile,
  type SaveClipboardImageAsTempFileArgs
} from './tauri-clipboard-image'
import { createRuntimeShellClient, createTauriShellClient, type ShellApi } from './shell-client'
import {
  createRuntimeUpdaterClient,
  createElectronUpdaterClient,
  createTauriUpdaterClient
} from './updater-client'
import { createRuntimeFsClient, createTauriFsClient, type FsApi } from './fs-client'
import { MCODE_APP_RESTART_ABORTED_EVENT } from '../../../shared/updater-renderer-events'

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
 * renderer boots under Tauri (no Electron preload). The clipboard and speech
 * bridges are merged over any existing `window.api`; under Electron this is a
 * no-op.
 */
export function installTauriWindowApiShim(): void {
  if (!isTauri()) {
    return
  }
  const existing = window.api as Partial<PreloadApi> | undefined

  // Runtime-selected bridges: every namespace resolves to its typed Tauri
  // client on first use; the Electron branches only run when the preload is
  // present (never under Tauri — these keep the call-shape parity tests
  // honest by mirroring the window.api surface verbatim).
  const shellBridge: ShellApi = createRuntimeShellClient({
    isTauri,
    electron: undefined as unknown as ShellApi,
    loadTauri: async () => createTauriShellClient((command, args) => invoke(command, args))
  })

  const updaterBridge = createRuntimeUpdaterClient({
    isTauri,
    electron: createElectronUpdaterClient(),
    loadTauri: async () =>
      createTauriUpdaterClient(
        (command, args) => invoke(command, args),
        (event, listener) => listen(event, listener),
        async () => {
          const context = (await invoke('app_get_launch_context')) as { version?: unknown }
          if (typeof context?.version !== 'string') {
            throw new Error('Invalid app_get_launch_context response')
          }
          return context.version
        },
        () => window.dispatchEvent(new Event(MCODE_APP_RESTART_ABORTED_EVENT))
      )
  })

  const fsBridge: FsApi = createRuntimeFsClient({
    isTauri,
    electron: undefined as unknown as FsApi,
    loadTauri: async () =>
      createTauriFsClient(
        (command, args) => invoke(command, args),
        (event, listener) => listen(event, listener)
      )
  })

  window.api = {
    ...existing,
    ui: {
      ...existing?.ui,
      ...tauriUiClipboardBridge
    },
    speech: {
      ...existing?.speech,
      ...tauriSpeechBridge
    },
    shell: {
      ...existing?.shell,
      ...shellBridge
    },
    updater: {
      ...existing?.updater,
      ...updaterBridge
    },
    fs: {
      ...existing?.fs,
      ...fsBridge
    }
  } as unknown as Window['api']
}
