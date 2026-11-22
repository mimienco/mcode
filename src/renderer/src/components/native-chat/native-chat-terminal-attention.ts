import { useEffect, useState } from 'react'
import type { AgentType } from '../../../../shared/agent-status-types'
import { isTextBlock, type NativeChatMessage } from '../../../../shared/native-chat-types'
import { stripAnsiEscapeSequences } from '../../../../shared/ansi-escape-sequences'

const TERMINAL_ATTENTION_POLL_MS = 500

export type NativeChatTerminalAttention = 'codex-hooks-review' | 'agent-auth-required'

/** Claude Code update banner state surfaced above the native chat view. */
export type NativeChatTerminalNotice =
  | { kind: 'update-available'; command?: string | null }
  | {
      kind: 'update-complete'
      features: number
      bugfixes: number
      otherChanges: number
      changelogUrl: string
    }

// Why: one-click updates run unsandboxed in a new tab, so only exact official
// Claude Code update commands are ever allowed through.
const ALLOWED_CLAUDE_UPDATE_COMMANDS: readonly string[] = ['claude update', 'claude migrate-installer']

export function isAllowedClaudeUpdateCommand(command: string): boolean {
  return ALLOWED_CLAUDE_UPDATE_COMMANDS.includes(command.trim())
}

function compactTerminalText(value: string): string {
  // Why: alternate-screen TUIs position words with cursor escapes, so prompt
  // detection cannot depend on literal spaces or terminal wrapping.
  return stripAnsiEscapeSequences(value).replace(/\s+/g, '').toLowerCase()
}

export function detectNativeChatTerminalAttention(
  screen: string | null | undefined,
  agent: AgentType
): NativeChatTerminalAttention | null {
  if (agent !== 'codex' || !screen) {
    return null
  }
  const text = compactTerminalText(screen)
  return text.includes('hooksneedreview') &&
    text.includes('trustallandcontinue') &&
    text.includes('continuewithouttrusting')
    ? 'codex-hooks-review'
    : null
}

export function detectNativeChatMessageAttention(
  messages: readonly NativeChatMessage[]
): NativeChatTerminalAttention | null {
  const latestAssistant = messages.findLast((message) => message.role === 'assistant')
  if (!latestAssistant) {
    return null
  }
  const text = latestAssistant.blocks
    .filter(isTextBlock)
    .map((block) => block.text)
    .join('\n')
    .trim()
  if (text.length === 0 || text.length > 320) {
    return null
  }
  return /(?:login|session)\s+(?:has\s+)?expired|not\s+(?:logged|signed)\s+in/i.test(text) &&
    /\/login|log\s*in|sign\s*in/i.test(text)
    ? 'agent-auth-required'
    : null
}

export function useNativeChatTerminalAttention(args: {
  agent: AgentType
  isVisible: boolean
  readTerminalScreen?: () => string | null
}): NativeChatTerminalAttention | null {
  const { agent, isVisible, readTerminalScreen } = args
  const [attention, setAttention] = useState<NativeChatTerminalAttention | null>(() =>
    isVisible ? detectNativeChatTerminalAttention(readTerminalScreen?.() ?? null, agent) : null
  )

  useEffect(() => {
    const refresh = (): void =>
      setAttention(
        isVisible ? detectNativeChatTerminalAttention(readTerminalScreen?.() ?? null, agent) : null
      )
    refresh()
    if (!isVisible || agent !== 'codex' || !readTerminalScreen) {
      return
    }
    // Why: the mounted xterm screen is transport-neutral for local, SSH, and
    // runtime-owned panes. Polling its small visible buffer also clears the
    // blocker as soon as Codex replaces the prompt after the user's choice.
    const interval = window.setInterval(refresh, TERMINAL_ATTENTION_POLL_MS)
    return () => window.clearInterval(interval)
  }, [agent, isVisible, readTerminalScreen])

  return isVisible && agent === 'codex' ? attention : null
}
