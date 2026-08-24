import { describe, expect, it } from 'vitest'
import { nativeChatSessionIsTranscriptOnly } from './native-chat-interaction-mode'

describe('nativeChatSessionIsTranscriptOnly', () => {
  it('makes a persisted transcript read-only when its restored PTY is only a shell', () => {
    expect(
      nativeChatSessionIsTranscriptOnly({
        persistedSession: { agent: 'codex', sessionId: 'old-session' },
        hasLiveAgentEvidence: false
      })
    ).toBe(true)
  })

  it('keeps a restored transcript interactive when the leaf has live agent evidence', () => {
    expect(
      nativeChatSessionIsTranscriptOnly({
        persistedSession: { agent: 'codex', sessionId: 'live-session' },
        hasLiveAgentEvidence: true
      })
    ).toBe(false)
  })

  it('does not affect ordinary live sessions without a persisted transcript fallback', () => {
    expect(
      nativeChatSessionIsTranscriptOnly({
        persistedSession: null,
        hasLiveAgentEvidence: false
      })
    ).toBe(false)
  })
})
