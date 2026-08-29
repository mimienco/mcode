import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ShutdownIntent } from './desktop-client'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'
import { runLifecycleSmoke } from './lifecycle-smoke-client'

const intent: ShutdownIntent =
  new URLSearchParams(window.location.search).get('intent') === 'restart' ? 'restart' : 'quit'
const lifecycle = createTauriDesktopAppLifecycleCapability(
  (command, args) => invoke(command, args),
  (event, listener) => listen(event, listener)
)

void runLifecycleSmoke({
  lifecycle,
  intent,
  stageCheckpoint: (checkpoint) =>
    invoke('app_stage_shutdown_checkpoint', { checkpoint }).then(() => undefined)
}).then(
  () => {
    document.getElementById('status')?.replaceChildren('Lifecycle listener ready')
  },
  (error) => {
    console.error('[smoke] lifecycle smoke failed', error)
  }
)
