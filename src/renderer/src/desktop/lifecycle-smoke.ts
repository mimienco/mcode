import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createDesktopInvoke, DesktopCommandError, type ShutdownIntent } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { runLifecycleSmoke } from './lifecycle-smoke-client'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'
import { createTauriFsClient } from './fs-client'
import { createTauriUiBridgeClient } from './tauri-ui-bridge-client'

const smokeParameters = new URLSearchParams(window.location.search)
const intent: ShutdownIntent = smokeParameters.get('intent') === 'restart' ? 'restart' : 'quit'
const shouldCheckUpdater = smokeParameters.get('updater') === 'true'
const shouldCheckWindowing = smokeParameters.get('windowing') === 'true'
const shouldCheckFilesystem = smokeParameters.get('filesystem') === 'true'
const shouldCheckCrossgrade = smokeParameters.get('crossgrade') === 'true'
const shouldCheckMenu = smokeParameters.get('menu') === 'true'
const desktopInvoke = createDesktopInvoke((command, args) => invoke(command, args))
const lifecycle = createTauriDesktopAppLifecycleCapability(desktopInvoke, (event, listener) =>
  listen(event, listener)
)
const fs = createTauriFsClient(
  (command, args) => invoke(command, args),
  (event, listener) => listen(event, listener)
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
      const savedPath = `${root}${separator}saved.txt`
      const missingPath = `${root}${separator}missing.txt`
      const entries = await fs.readDir({ dirPath: root })
      const initialContent = await fs.readFile({ filePath: childPath })
      const metadata = await fs.stat({ filePath: childPath })
      const missingExists = await fs.pathExists({
        filePath: missingPath
      })
      let outsideDenied = false
      let outsideWriteDenied = false
      try {
        await fs.pathExists({ filePath: `${root}-outside` })
      } catch (error) {
        outsideDenied = error instanceof DesktopCommandError && error.code === 'fs_path_denied'
      }
      try {
        await fs.writeFileAtomic({
          filePath: `${root}-outside${separator}denied.txt`,
          content: 'denied'
        })
      } catch (error) {
        outsideWriteDenied = error instanceof DesktopCommandError && error.code === 'fs_path_denied'
      }
      await fs.writeFileAtomic({
        filePath: savedPath,
        content: 'atomic save'
      })
      await fs.writeFileAtomic({
        filePath: childPath,
        content: 'atomic replacement'
      })
      const savedContent = await fs.readFile({ filePath: savedPath })
      const replacedContent = await fs.readFile({ filePath: childPath })
      if (
        !entries.some((entry) => entry.name === 'visible.txt' && !entry.isDirectory) ||
        initialContent.content !== 'visible' ||
        initialContent.isBinary ||
        metadata.size !== 7 ||
        metadata.isDirectory ||
        missingExists ||
        !outsideDenied ||
        !outsideWriteDenied ||
        savedContent.content !== 'atomic save' ||
        savedContent.isBinary ||
        replacedContent.content !== 'atomic replacement' ||
        replacedContent.isBinary
      ) {
        throw new Error('filesystem smoke assertions failed')
      }
      await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true })
      document.getElementById('status')?.replaceChildren('Filesystem smoke passed')
      return
    }
    if (shouldCheckCrossgrade) {
      // Crossgrade round-trip (D-012 explicit-command surface): status first,
      // then the full migrate pipeline (plan → stage → verify → commit →
      // discard) against the fixture COPY configured via
      // MCODE_TAURI_SETTINGS_PATH + APPDATA overrides at launch. The source
      // dir is never modified — status is read-only and migrate only copies.
      // SmokeTestMode discipline: the RENDERER writes the verdict marker via
      // the fs client; Rust only polls it (pty-smoke pattern).
      const crossgradeMarker = smokeParameters.get('marker')
      if (!crossgradeMarker) {
        throw new Error('crossgrade smoke marker path is missing')
      }
      const writeVerdict = (verdict: string): Promise<void> =>
        fs.writeFileAtomic({ filePath: crossgradeMarker, content: verdict })
      try {
        const status: unknown = await desktopInvoke('settings_crossgrade_status')
        const discovered =
          typeof status === 'object' && status !== null && 'discovered' in status
            ? status.discovered === true
            : false
        if (!discovered) {
          throw new Error('crossgrade smoke: source not discovered')
        }
        const result: unknown = await desktopInvoke('settings_crossgrade_migrate')
        const migratedKeys =
          typeof result === 'object' && result !== null && 'migratedKeys' in result
            ? result.migratedKeys
            : undefined
        const telemetryPreserved =
          typeof result === 'object' && result !== null && 'telemetryPreserved' in result
            ? result.telemetryPreserved
            : undefined
        const stagingDiscarded =
          typeof result === 'object' && result !== null && 'stagingDiscarded' in result
            ? result.stagingDiscarded
            : undefined
        if (
          typeof migratedKeys !== 'number' ||
          migratedKeys <= 0 ||
          telemetryPreserved !== true ||
          stagingDiscarded !== true
        ) {
          throw new Error(`crossgrade smoke: migrate result invalid: ${JSON.stringify(result)}`)
        }
        await writeVerdict('passed')
        document.getElementById('status')?.replaceChildren('Crossgrade smoke passed')
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        await writeVerdict(`renderer-failed: ${message}`)
        document.getElementById('status')?.replaceChildren(`Crossgrade smoke failed: ${message}`)
        throw error
      }
      await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true })
      return
    }
    if (shouldCheckMenu) {
      // Menu smoke (app_menu.rs dispatch): subscribe every menu-driven event
      // through the typed UI bridge, then drive the REAL click dispatcher
      // (handle_menu_event) through the debug-only ui_smoke_trigger_menu_item
      // command — the exact path a native muda click takes. The RENDERER
      // writes the verdict marker via the fs client; Rust only polls it.
      const menuMarker = smokeParameters.get('marker')
      if (!menuMarker) {
        throw new Error('menu smoke marker path is missing')
      }
      const writeVerdict = (verdict: string): Promise<void> =>
        fs.writeFileAtomic({ filePath: menuMarker, content: verdict })
      try {
        const ui = createTauriUiBridgeClient(
          (command, args) => invoke(command, args),
          (event, listener) => listen(event, listener)
        )
        const received: string[] = []
        const expect = (name: string): void => {
          if (received.includes(name)) {
            throw new Error(`menu smoke: duplicate event ${name}`)
          }
          received.push(name)
        }
        const offs = [
          ui.onAppMenuPaste(() => expect('paste')),
          ui.onAppMenuSelectionAction((action) => expect(`selection:${action}`)),
          ui.onTerminalZoom((direction) => expect(`zoom:${direction}`)),
          ui.onToggleLeftSidebar(() => expect('sidebar:left')),
          ui.onToggleRightSidebar(() => expect('sidebar:right')),
          ui.onToggleStatusBar(() => expect('statusBar')),
          ui.onToggleAppearanceSetting((key) => expect(`appearance:${key}`)),
          ui.onCheckForUpdates(() => expect('checkForUpdates'))
        ]
        const expected = [
          'paste',
          'selection:copy',
          'selection:select-all',
          'zoom:in',
          'zoom:out',
          'zoom:reset',
          'sidebar:left',
          'sidebar:right',
          'statusBar',
          'appearance:showTasksButton',
          'checkForUpdates'
        ]
        // Dispatch through the real dispatcher: paste, copy, select-all,
        // zoom in/out/reset, both sidebars, status bar, one appearance
        // checkbox, check-for-updates. Undo/redo share the void-event shape
        // with paste (exercised by the bridge unit test).
        const items = [
          'mcode-edit-paste',
          'mcode-edit-copy',
          'mcode-edit-select-all',
          'mcode-view-zoom-in',
          'mcode-view-zoom-out',
          'mcode-view-zoom-reset',
          'mcode-view-sidebar-left',
          'mcode-view-sidebar-right',
          'mcode-view-status-bar',
          'mcode-view-tasks-button',
          'mcode-help-check-updates'
        ]
        for (const item of items) {
          await desktopInvoke('ui_smoke_trigger_menu_item', { itemId: item })
        }
        // All events are emitted synchronously through the Rust dispatcher
        // and delivered over the Tauri event bus; give the listeners a beat.
        {
          const { promise, resolve } = Promise.withResolvers<void>()
          setTimeout(resolve, 500)
          await promise
        }
        for (const off of offs) {
          off()
        }
        const missing = expected.filter((name) => !received.includes(name))
        if (missing.length > 0) {
          throw new Error(`menu smoke: events not received: ${missing.join(', ')}`)
        }
        await writeVerdict('passed')
        document.getElementById('status')?.replaceChildren('Menu smoke passed')
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        await writeVerdict(`renderer-failed: ${message}`)
        document.getElementById('status')?.replaceChildren(`Menu smoke failed: ${message}`)
        throw error
      }
      await desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true })
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
    if (shouldCheckFilesystem) {
      const root = smokeParameters.get('root')
      if (root) {
        const separator = root.includes('\\') ? '\\' : '/'
        const message = error instanceof Error ? error.message : String(error)
        void fs
          .writeFileAtomic({
            filePath: `${root}${separator}.mcode-filesystem-smoke-error.txt`,
            content: message
          })
          .then(() => desktopInvoke('window_set_minimize_to_tray_on_close', { enabled: true }))
          .catch((reportError: unknown) => {
            console.error('[smoke] failed to report filesystem smoke error', reportError)
          })
      }
    }
  })
