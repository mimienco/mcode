import { describe, expect, it } from 'vitest'
import type { NativeChatMessage } from '../../../../shared/native-chat-types'
import { detectNativeChatMessageAttention } from './native-chat-terminal-attention'

function message(role: NativeChatMessage['role'], text: string): NativeChatMessage {
  return {
    id: `${role}:${text}`,
    role,
    blocks: [{ type: 'text', text }],
    timestamp: 1,
    source: 'transcript'
  }
}

describe('detectNativeChatMessageAttention', () => {
  it.each([
    'Login expired · Please run /login',
    'Not logged in · Please run /login',
    'Session has expired. Sign in again.'
  ])('detects an actionable short authentication failure: %s', (text) => {
    expect(detectNativeChatMessageAttention([message('assistant', text)])).toBe(
      'agent-auth-required'
    )
  })

  it('uses only the latest assistant result so a recovered session clears the blocker', () => {
    expect(
      detectNativeChatMessageAttention([
        message('assistant', 'Login expired · Please run /login'),
        message('user', '/login'),
        message('assistant', 'Ready to continue.')
      ])
    ).toBeNull()
  })

  it('does not treat ordinary discussion of authentication as a blocker', () => {
    expect(
      detectNativeChatMessageAttention([
        message('assistant', 'I can update the login flow and add a session-expired test.')
      ])
    ).toBeNull()
  })
})
