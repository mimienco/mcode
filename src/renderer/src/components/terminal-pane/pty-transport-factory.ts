// Runtime-selected transport seam (docs/migration/TERMINAL_UI_INTEGRATION.md
// §2.1): the single edit point where the Electron IPC transport and the Tauri
// PTY transport diverge. Selects exactly once per construction — there is no
// runtime migration path between transports for a live pane (§3.5).
import { isTauri } from '@tauri-apps/api/core'
import { createIpcPtyTransport } from './pty-transport'
import { createTauriPtyTransport } from './pty-transport-tauri'
import type { IpcPtyTransportOptions, PtyTransport } from './pty-transport-types'

export function createLocalPtyTransport(
  opts: IpcPtyTransportOptions,
  deps?: { isTauri?: () => boolean }
): PtyTransport {
  return (deps?.isTauri ?? isTauri)() ? createTauriPtyTransport(opts) : createIpcPtyTransport(opts)
}
