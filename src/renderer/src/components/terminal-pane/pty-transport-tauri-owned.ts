//! Owned-handler registry for the Tauri PTY transport. Tracks which
//! dispatcher entries belong to the current binding so a rebind without
//! detach can never delete a live pane's handlers (TERMINAL_UI_INTEGRATION.md
//! §2.1 ownership rule).

import type { PtyDataMeta } from './pty-dispatcher'

export type OwnedDataAndReplay = {
  data: (data: string, meta?: PtyDataMeta) => void
  replay: (data: string) => void
  writeUnavailable: () => void
}

import { ptyDataHandlers, ptyReplayHandlers, ptyWriteUnavailableHandlers } from './pty-dispatcher'

/** Deletes only the dispatcher entries this binding owns (§2.1 ownership rule). */
export function unregisterPtyDataAndStatusHandlers(
  id: string,
  ownedDataAndReplayHandlers: Map<string, OwnedDataAndReplay>
): void {
  const owned = ownedDataAndReplayHandlers.get(id)
  if (owned) {
    if (ptyDataHandlers.get(id) === owned.data) {
      ptyDataHandlers.delete(id)
    }
    if (ptyReplayHandlers.get(id) === owned.replay) {
      ptyReplayHandlers.delete(id)
    }
    if (ptyWriteUnavailableHandlers.get(id) === owned.writeUnavailable) {
      ptyWriteUnavailableHandlers.delete(id)
    }
  }
  ownedDataAndReplayHandlers.delete(id)
}
