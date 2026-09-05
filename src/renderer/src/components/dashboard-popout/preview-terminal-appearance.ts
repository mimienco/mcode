import { useEffect, type RefObject } from 'react'
import type { Terminal } from '@xterm/xterm'
import { buildPreviewAppearanceOptions } from './preview-terminal-options'
import { syncPreviewTerminalLigatures } from './preview-terminal-ligatures'

type PreviewTerminalSettings = Parameters<typeof buildPreviewAppearanceOptions>[0]

/**
 * Why: appearance settings must land on the open terminal, and the OS input
 * source can flip Option-as-Alt with no settings change at all. A remount
 * would reconnect the pty and repaint the agent's screen from a new snapshot.
 */
export function usePreviewTerminalAppearance(
  terminalRef: RefObject<Terminal | null>,
  settings: PreviewTerminalSettings,
  macOptionAsAlt: boolean | string
): void {
  useEffect(() => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    Object.assign(
      terminal.options,
      buildPreviewAppearanceOptions(settings, macOptionAsAlt === 'true')
    )
    syncPreviewTerminalLigatures(terminal, settings)
  }, [terminalRef, settings, macOptionAsAlt])
}
