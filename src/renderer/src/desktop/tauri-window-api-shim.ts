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
import { createTauriUiBridgeClient } from './tauri-ui-bridge-client'
import { tauriTerminalPreviewClient } from './tauri-terminal-preview-client'
import { createTauriAppClient } from './tauri-app-client'
import { createUnportedNamespace } from './tauri-unported-namespace'
import {
  createSettingsDocumentBridge,
  createSettingsClient,
  createKeybindingsClient,
  createPersistedUiClient
} from './tauri-settings-document-client'

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
 * Merges a plain bridge over an optional existing namespace WITHOUT spreading:
 * spreading a Proxy client materializes only its own properties and silently
 * drops every get-trap member (the ui passthrough listeners). The bridge
 * layer wins over existing (spread precedence); existing answers for
 * everything the bridge leaves undefined.
 */
function mergeNamespace(
  existing: Record<string, unknown> | undefined,
  bridge: Record<string, unknown>
): Record<string, unknown> {
  // Spread semantics `{ ...existing, ...bridge }` WITHOUT materializing the
  // bridge: spreading a Proxy client would drop every get-trap member (the
  // ui passthrough listeners). The bridge layer wins over existing for keys
  // it owns or traps; existing answers for everything the bridge leaves
  // undefined (a plain-object layer returning undefined must not shadow a
  // later layer's trap — that is how clipboard used to hide the passthrough).
  const layers: Record<string, unknown>[] = [bridge, ...(existing ? [existing] : [])]
  return new Proxy({} as Record<string, unknown>, {
    get(_target, method: string) {
      if (typeof method !== 'string') {
        return undefined
      }
      for (const layer of layers) {
        const value = (layer as Record<string, unknown>)[method]
        if (value !== undefined) {
          return value
        }
        if (method in layer) {
          return value
        }
      }
      return undefined
    }
  })
}

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

  const settingsDocumentBridge = createSettingsDocumentBridge((command, args) =>
    invoke(command, args)
  )
  const settingsBridge = createSettingsClient(settingsDocumentBridge)
  const keybindingsBridge = createKeybindingsClient(settingsDocumentBridge)
  const persistedUiBridge = createPersistedUiClient(settingsDocumentBridge)

  const shimmedApi = {
    app: mergeNamespace(
      existing?.app,
      createTauriAppClient((command, args) => invoke(command, args))
    ),
    settings: mergeNamespace(existing?.settings, settingsBridge),
    keybindings: mergeNamespace(existing?.keybindings, keybindingsBridge),
    ui: mergeNamespace(
      mergeNamespace(mergeNamespace(existing?.ui, tauriUiClipboardBridge), persistedUiBridge),
      createTauriUiBridgeClient(
        (command, args) => invoke(command, args),
        (event, listener) => listen(event, listener)
      )
    ),
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
    },
    terminalPreview: {
      ...existing?.terminalPreview,
      ...tauriTerminalPreviewClient
    }
  } as unknown as Window['api']
  // Any namespace without a native bridge resolves through the presence
  // shim so bootstrap/effect dereferences cannot crash the React tree.
  window.api = new Proxy(shimmedApi, {
    get(target, namespace: string) {
      if (namespace in target) {
        return target[namespace as keyof typeof target]
      }
      return createUnportedNamespace(namespace)
    }
  }) as unknown as Window['api']
}
