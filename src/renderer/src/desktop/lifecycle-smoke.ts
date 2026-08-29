import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { createDesktopInvoke, type ShutdownIntent } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { runLifecycleSmoke } from './lifecycle-smoke-client'

const intent: ShutdownIntent =
  new URLSearchParams(window.location.search).get('intent') === 'restart' ? 'restart' : 'quit'
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
  () => {
    document.getElementById('status')?.replaceChildren('Lifecycle listener ready')
  },
  (error) => {
    console.error('[smoke] lifecycle smoke failed', error)
  }
)
