// @vitest-environment happy-dom

import { act, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  detectNativeChatTerminalAttention,
  detectNativeChatTerminalNotice,
  useNativeChatTerminalAttention
} from './native-chat-terminal-attention'

const CODEX_HOOK_REVIEW_PROMPT = [
  '\u001b[33mHooks need review\u001b[0m',
  '2 hooks are new or changed.',
  '1. Review hooks',
  '2. Trust all and continue',
  "3. Continue without trusting (hooks won't run)",
  'Press enter to confirm or esc to go back'
].join('\r\n')

describe('native chat terminal attention', () => {
  afterEach(() => vi.useRealTimers())

  it('recognizes the Codex hook-review prompt across ANSI and line breaks', () => {
    expect(detectNativeChatTerminalAttention(CODEX_HOOK_REVIEW_PROMPT, 'codex')).toBe(
      'codex-hooks-review'
    )
  })

  it('does not classify ordinary output or another agent as a Codex blocker', () => {
    expect(detectNativeChatTerminalAttention('Review hooks before release.', 'codex')).toBeNull()
    expect(detectNativeChatTerminalAttention(CODEX_HOOK_REVIEW_PROMPT, 'claude')).toBeNull()
  })

  it('promotes Claude terminal authentication failures into Chat', () => {
    expect(detectNativeChatTerminalAttention('Not logged in · Run /login', 'claude')).toBe(
      'agent-auth-required'
    )
  })

  it('parses a completed Claude update with its changelog counts', () => {
    expect(
      detectNativeChatTerminalNotice(
        'Updated to latest. Got 10 features, 34 bugfixes, and 17 other changes.\ncode.claude.com/docs/en/changelog for details',
        'claude'
      )
    ).toEqual({
      kind: 'update-complete',
      features: 10,
      bugfixes: 34,
      otherChanges: 17,
      changelogUrl: 'https://code.claude.com/docs/en/changelog'
    })
  })

  it.each([
    ['Update available! Run: claude update', 'claude update'],
    ['Update available! Run: brew upgrade claude-code', 'brew upgrade claude-code'],
    [
      'Update available! Run: winget upgrade Anthropic.ClaudeCode',
      'winget upgrade Anthropic.ClaudeCode'
    ]
  ])('extracts an allowlisted one-click update command from %s', (screen, command) => {
    expect(detectNativeChatTerminalNotice(screen, 'claude')).toEqual({
      kind: 'update-available',
      command
    })
  })

  it('never turns untrusted terminal output into an executable update action', () => {
    expect(
      detectNativeChatTerminalNotice('Update available! Run: rm -rf important', 'claude')
    ).toEqual({ kind: 'update-available', command: null })
  })

  it('clears the blocker after the terminal prompt is resolved', () => {
    vi.useFakeTimers()
    let screen = CODEX_HOOK_REVIEW_PROMPT
    const readTerminalScreen = (): string => screen
    const { result } = renderHook(() =>
      useNativeChatTerminalAttention({
        agent: 'codex',
        isVisible: true,
        readTerminalScreen
      })
    )

    expect(result.current).toBe('codex-hooks-review')
    screen = 'Codex is ready'
    act(() => vi.advanceTimersByTime(500))
    expect(result.current).toBeNull()
  })
})
