import type { TuiAgent } from '../../../../shared/tui-agent'
import type { NativeChatSession } from '../../../../shared/native-chat-types'
import type { NativeChatContextMenuActions } from './use-native-chat-context-menu'

export type NativeChatViewProps = {
  /** The terminal tab hosting the agent. paneKey is `${tabId}:${leafId}`. */
  terminalTabId: string
  /** Whether the hosted terminal surface is currently visible. */
  isVisible: boolean
  /** Specific split leaf this chat surface replaces. */
  paneKey?: string
  /** PTY bound to `paneKey`, used for composer and interactive-card sends. */
  targetPtyId?: string | null
  /** Launch-time agent hint from the TerminalTab, when MCode started one. */
  launchAgent?: TuiAgent | null
  /** Trusted title/foreground fallback for manually-started agents. */
  resolvedAgent?: TuiAgent | null
  /** Stable provider identity persisted with a restored single-pane tab. */
  persistedSession?: {
    agent: Extract<TuiAgent, 'claude' | 'codex'>
    sessionId: string
  } | null
  /** A persisted transcript can outlive its agent PTY; keep that surface
   *  readable without routing composer input into the replacement shell. */
  transcriptOnly?: boolean
  /** Return this pane to the hosted terminal surface. */
  onSwitchToTerminal?: () => void
  /** Current xterm screen reader used to recover agent-reported session state. */
  readTerminalScreen?: () => string | null
  contextMenuActions?: Omit<NativeChatContextMenuActions, 'onPaste'>
}

export type NativeChatResolvedViewProps = {
  paneKey: string
  agent: NativeChatSession['agent']
  sessionId: string | null
  transcriptPath: string | null
  transcriptOnly: boolean
  isVisible: boolean
  targetPtyId: string | null
  terminalTabId: string
  onSwitchToTerminal?: () => void
  readTerminalScreen?: () => string | null
  contextMenuActions?: Omit<NativeChatContextMenuActions, 'onPaste'>
}
