import { createDesktopInvoke } from './desktop-client'
import { getDefaultUIState } from '../../../shared/constants'
import type { PersistedUIState } from '../../../shared/persisted-ui-state-types'
import { homeDir } from '@tauri-apps/api/path'
import { resolveTauriSettings } from './tauri-settings-defaults'

// ─── Settings-document bridge ────────────────────────────────────────────────
//
// Backed by settings_get_document / settings_set_document (Rust
// settings_document_commands.rs): the SAME mcode-data.json document the
// settings service owns — 'settings' key = GlobalSettings, 'ui' key =
// PersistedUIState. Merge-patch writes go through the service's write lock
// + atomic-file commit.
//
// Contract notes vs the Electron preload:
//  - settings.getSync() returned a blocking IPC read; under Tauri there is
//    no sync bridge, so it reads the last cached document (null before the
//    first async get — the preload's own `GlobalSettings | null` contract).
//  - onChanged listeners fire on every successful local set; out-of-band
//    change events (View > Appearance toggles) have no Tauri emitter yet.

type DocumentListener = (document: Record<string, unknown>) => void

export type SettingsDocumentBridge = {
  getDocument: () => Promise<Record<string, unknown>>
  setDocument: (patch: Record<string, unknown>) => Promise<Record<string, unknown>>
  cache: () => Record<string, unknown> | null
  onDocumentChanged: (listener: DocumentListener) => () => void
}

export function createSettingsDocumentBridge(
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>
): SettingsDocumentBridge {
  const call = createDesktopInvoke(invoke)
  let cached: Record<string, unknown> | null = null
  let revision = 0
  let writes: Promise<void> = Promise.resolve()
  const listeners = new Set<DocumentListener>()

  const publish = (document: Record<string, unknown>): Record<string, unknown> => {
    cached = document
    for (const listener of listeners) {
      listener(document)
    }
    return document
  }

  return {
    getDocument: async () => {
      await writes
      const requestRevision = ++revision
      const document = (await call('settings_get_document')) as Record<string, unknown>
      return requestRevision === revision ? publish(document) : document
    },
    setDocument: (patch) => {
      revision++
      const result = writes.then(async () => {
        const requestRevision = ++revision
        const document = (await call('settings_set_document', { patch })) as Record<string, unknown>
        return requestRevision === revision ? publish(document) : document
      })
      writes = result.then(
        () => undefined,
        () => undefined
      )
      return result
    },
    cache: () => cached,
    onDocumentChanged: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    }
  }
}

function section(document: Record<string, unknown> | null, key: string): Record<string, unknown> {
  const value = document?.[key]
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

/** settings.* — GlobalSettings under the document's 'settings' key. */
export function createSettingsClient(
  bridge: SettingsDocumentBridge,
  getHomeDirectory: () => Promise<string> = homeDir
): Record<string, unknown> {
  let home: string | null = null
  let loadingHome: Promise<string> | null = null
  const ensureHome = (): Promise<string> => {
    loadingHome ??= getHomeDirectory()
      .then((value) => {
        home = value
        return value
      })
      .catch((error: unknown) => {
        loadingHome = null
        throw error
      })
    return loadingHome
  }
  const settingsSection = (document: Record<string, unknown>) =>
    resolveTauriSettings(document, home!)
  return {
    get: async () => {
      const [document] = await Promise.all([bridge.getDocument(), ensureHome()])
      return settingsSection(document)
    },
    getSync: () =>
      bridge.cache() === null || home === null ? null : settingsSection(bridge.cache()!),
    set: async (updates: Record<string, unknown>) => {
      await ensureHome()
      const document = await bridge.setDocument({ settings: updates })
      return settingsSection(document)
    },
    setActiveRuntimeEnvironmentPreference: async (args: { environmentId: string | null }) => {
      await ensureHome()
      const document = await bridge.setDocument({
        settings: { activeRuntimeEnvironmentId: args.environmentId }
      })
      return settingsSection(document)
    },
    updatePRBotAuthorOverride: async (args: { author: string; isBot: boolean }) => {
      await ensureHome()
      const document = await bridge.setDocument({
        settings: { prBotAuthorOverride: args }
      })
      return settingsSection(document)
    },
    listFonts: async () => {
      await bridge.getDocument()
      return []
    },
    previewGhosttyImport: async () => {
      throw new Error('Ghostty import is not available under Tauri yet')
    },
    previewWarpThemeImport: async () => {
      throw new Error('Warp import is not available under Tauri yet')
    },
    onChanged: (callback: (updates: Partial<Record<string, unknown>>) => void) => {
      let active = true
      const detach = bridge.onDocumentChanged((document) => {
        void ensureHome()
          .then(() => {
            if (active) {
              callback(settingsSection(document) as unknown as Record<string, unknown>)
            }
          })
          .catch((error: unknown) =>
            console.error('[settings] Could not load settings defaults', error)
          )
      })
      return () => {
        active = false
        detach()
      }
    }
  }
}

/** keybindings.* — snapshot under the document's 'keybindings' key. */
export function createKeybindingsClient(bridge: SettingsDocumentBridge): Record<string, unknown> {
  const keybindingsSection = (): Record<string, unknown> => section(bridge.cache(), 'keybindings')
  return {
    get: async () => {
      await bridge.getDocument()
      return keybindingsSection()
    },
    ensureFile: async () => {
      await bridge.getDocument()
      return keybindingsSection()
    },
    setAction: async (args: { actionId: string; bindings: string[] | null }) => {
      const snapshot = keybindingsSection()
      const bindings = (snapshot.bindings ?? {}) as Record<string, unknown>
      const document = await bridge.setDocument({
        keybindings: { bindings: { ...bindings, [args.actionId]: args.bindings } }
      })
      return section(document, 'keybindings')
    },
    reload: async () => {
      await bridge.getDocument()
      return keybindingsSection()
    },
    openFile: async () => {
      await bridge.getDocument()
      return keybindingsSection()
    },
    revealFile: async () => {
      await bridge.getDocument()
      return keybindingsSection()
    },
    onChanged: (callback: (snapshot: Record<string, unknown>) => void) =>
      bridge.onDocumentChanged((document) => callback(section(document, 'keybindings')))
  }
}

/**
 * ui.get/set — PersistedUIState under the document's 'ui' key. Missing keys
 * fall through to the store's own defaults (getDefaultUIState) so hydration
 * layers cleanly; set persists a merge patch (persistence slice: the same
 * document the crossgrade migrates ui.windowBounds into).
 */
export function createPersistedUiClient(bridge: SettingsDocumentBridge): Record<string, unknown> {
  return {
    get: async () => {
      await bridge.getDocument()
      return { ...getDefaultUIState(), ...section(bridge.cache(), 'ui') }
    },
    set: async (updates: Partial<PersistedUIState>) => {
      const document = await bridge.setDocument({ ui: updates })
      return { ...getDefaultUIState(), ...section(document, 'ui') }
    }
  }
}
