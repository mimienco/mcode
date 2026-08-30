import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { createDesktopInvoke, type ShutdownIntent } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { runLifecycleSmoke } from './lifecycle-smoke-client'

const smokeParameters = new URLSearchParams(window.location.search)
const intent: ShutdownIntent = smokeParameters.get('intent') === 'restart' ? 'restart' : 'quit'
const shouldCheckUpdater = smokeParameters.get('updater') === 'true'
const desktopInvoke = createDesktopInvoke((command, args) => invoke(command, args))
const lifecycle = createTauriDesktopAppLifecycleCapability(desktopInvoke, (event, listener) =>
  listen(event, listener)
)

void runLifecycleSmoke({
  lifecycle,
  intent,
  stageCheckpoint: (checkpoint) =>
    desktopInvoke('app_stage_shutdown_checkpoint', { checkpoint }).then(() => undefined)
}).then(
  async () => {
    document.getElementById('status')?.replaceChildren('Lifecycle listener ready')
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
  },
  (error) => {
    console.error('[smoke] lifecycle smoke failed', error)
  }
)
