import { useEffect, useState } from 'react'
import type { AgentType } from '../../../../shared/agent-status-types'
import { isTextBlock, type NativeChatMessage } from '../../../../shared/native-chat-types'
import { stripAnsiEscapeSequences } from '../../../../shared/ansi-escape-sequences'

const TERMINAL_ATTENTION_POLL_MS = 500

export type NativeChatTerminalAttention = 'codex-hooks-review' | 'agent-auth-required'

export type NativeChatTerminalNotice =
  | {
      kind: 'update-available'
      command: string | null
    }
  | {
      kind: 'update-complete'
      features: number
      bugfixes: number
      otherChanges: number
      changelogUrl: 'https://code.claude.com/docs/en/changelog'
    }

type NativeChatTerminalState = {
  attention: NativeChatTerminalAttention | null
  notice: NativeChatTerminalNotice | null
}

const EMPTY_TERMINAL_STATE: NativeChatTerminalState = { attention: null, notice: null }
const CLAUDE_CHANGELOG_URL = 'https://code.claude.com/docs/en/changelog' as const
const CLAUDE_UPDATE_COMMANDS = [
  /^claude update$/i,
  /^brew upgrade (?:--cask )?claude-code(?:@latest)?$/i,
  /^winget upgrade (?:--id )?Anthropic\.ClaudeCode$/i
]

function readNativeChatTerminalState(args: {
  agent: AgentType
  isVisible: boolean
  readTerminalScreen?: () => string | null
}): NativeChatTerminalState {
  if (!args.isVisible) {
    return EMPTY_TERMINAL_STATE
  }
  const screen = args.readTerminalScreen?.() ?? null
  return {
    attention: detectNativeChatTerminalAttention(screen, args.agent),
    notice: detectNativeChatTerminalNotice(screen, args.agent)
  }
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
  if (!screen) {
    return null
  }
  const text = compactTerminalText(screen)
  if (
    /not(?:logged|signed)in|(?:login|session)(?:has)?expired/.test(text) &&
    /\/login|runlogin|signinagain/.test(text)
  ) {
    return 'agent-auth-required'
  }
  return agent === 'codex' &&
    text.includes('hooksneedreview') &&
    text.includes('trustallandcontinue') &&
    text.includes('continuewithouttrusting')
    ? 'codex-hooks-review'
    : null
}

export function isAllowedClaudeUpdateCommand(command: string): boolean {
  return CLAUDE_UPDATE_COMMANDS.some((pattern) => pattern.test(command.trim()))
}

export function detectNativeChatTerminalNotice(
  screen: string | null | undefined,
  agent: AgentType
): NativeChatTerminalNotice | null {
  if (agent !== 'claude' || !screen) {
    return null
  }
  const text = stripAnsiEscapeSequences(screen).replace(/\r/g, '')
  const complete = text.match(
    /Updated\s+to\s+latest\.\s*Got\s+(\d+)\s+features,\s*(\d+)\s+bugfixes,\s*and\s*(\d+)\s+other\s+changes\./i
  )
  if (complete) {
    return {
      kind: 'update-complete',
      features: Number(complete[1]),
      bugfixes: Number(complete[2]),
      otherChanges: Number(complete[3]),
      changelogUrl: CLAUDE_CHANGELOG_URL
    }
  }
  const available = text.match(/Update\s+available!?\s*Run:\s*([^\n]+)/i)
  if (!available) {
    return null
  }
  const candidate = available[1]?.trim() ?? ''
  return {
    kind: 'update-available',
    command: isAllowedClaudeUpdateCommand(candidate) ? candidate : null
  }
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
  return useNativeChatTerminalState(args).attention
}

export function useNativeChatTerminalState(args: {
  agent: AgentType
  isVisible: boolean
  readTerminalScreen?: () => string | null
}): NativeChatTerminalState {
  const { agent, isVisible, readTerminalScreen } = args
  const [state, setState] = useState<NativeChatTerminalState>(() =>
    readNativeChatTerminalState(args)
  )

  useEffect(() => {
    const refresh = (): void => {
      const next = readNativeChatTerminalState({ agent, isVisible, readTerminalScreen })
      setState((current) => (JSON.stringify(current) === JSON.stringify(next) ? current : next))
    }
    refresh()
    if (!isVisible || !readTerminalScreen) {
      return
    }
    // Why: the mounted xterm screen is transport-neutral for local, SSH, and
    // runtime-owned panes. Polling its small visible buffer also clears the
    // blocker as soon as Codex replaces the prompt after the user's choice.
    const interval = window.setInterval(refresh, TERMINAL_ATTENTION_POLL_MS)
    return () => window.clearInterval(interval)
  }, [agent, isVisible, readTerminalScreen])

  return isVisible ? state : EMPTY_TERMINAL_STATE
}
