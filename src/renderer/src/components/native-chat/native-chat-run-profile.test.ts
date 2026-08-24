import { describe, expect, it } from 'vitest'
import { buildNativeChatRunProfileSnapshot } from './native-chat-run-profile'

describe('native Chat run profile', () => {
  it('marks Codex controls live from the App Server capability snapshot', () => {
    const snapshot = buildNativeChatRunProfileSnapshot({
      agent: 'codex',
      permission: 'ask',
      interactionMode: 'build',
      codexLiveSnapshot: {
        available: true,
        models: [],
        controls: {
          permissions: ['ask', 'auto-edits'],
          interactionModes: ['plan', 'build']
        },
        current: {
          model: 'gpt-5.6-sol',
          permission: 'auto-edits',
          interactionMode: 'plan'
        }
      }
    })

    expect(snapshot.map((descriptor) => descriptor.applyScope)).toEqual([
      'current-session',
      'current-session'
    ])
    expect(snapshot[0]?.kind).toMatchObject({ currentValue: 'auto-edits' })
    expect(snapshot[1]?.kind).toMatchObject({ currentValue: 'plan' })
    expect(
      snapshot[0]?.kind.type === 'select'
        ? snapshot[0].kind.choices.find((choice) => choice.value === 'bypass')?.disabled
        : undefined
    ).toBe(true)
  })

  it('keeps Cursor auto-edits disabled and labels launch-only state truthfully', () => {
    const snapshot = buildNativeChatRunProfileSnapshot({
      agent: 'cursor',
      permission: 'ask',
      interactionMode: 'build',
      codexLiveSnapshot: { available: false }
    })

    expect(snapshot[0]).toMatchObject({ applyScope: 'next-session', valueSource: 'default' })
    expect(
      snapshot[0]?.kind.type === 'select'
        ? snapshot[0].kind.choices.find((choice) => choice.value === 'auto-edits')
        : undefined
    ).toMatchObject({ disabled: true })
  })

  it('shows Cursor Ask rather than claiming a global unsupported auto-edits intent applied', () => {
    const snapshot = buildNativeChatRunProfileSnapshot({
      agent: 'cursor',
      permission: 'auto-edits',
      interactionMode: 'build',
      codexLiveSnapshot: { available: false }
    })

    expect(snapshot[0]?.kind).toMatchObject({ currentValue: 'ask' })
  })

  it('does not claim an interactive-only Plan fallback changed a Grok task', () => {
    const snapshot = buildNativeChatRunProfileSnapshot({
      agent: 'grok',
      permission: 'ask',
      interactionMode: 'plan',
      codexLiveSnapshot: { available: false }
    })

    expect(snapshot[1]?.kind).toMatchObject({ currentValue: 'build' })
    expect(
      snapshot[1]?.kind.type === 'select'
        ? snapshot[1].kind.choices.find((choice) => choice.value === 'plan')
        : undefined
    ).toMatchObject({
      disabled: true,
      disabledReason: expect.stringContaining('/plan or Shift+Tab')
    })
  })

  it('marks custom provider launch arguments as an unknown override', () => {
    const snapshot = buildNativeChatRunProfileSnapshot({
      agent: 'claude',
      permission: 'ask',
      interactionMode: 'build',
      codexLiveSnapshot: { available: false },
      customLaunchOverride: true
    })

    expect(snapshot[0]).toMatchObject({
      settable: false,
      description: expect.stringContaining('Custom provider launch arguments')
    })
    expect(snapshot[0]?.kind).toMatchObject({ currentValue: undefined })
    expect(snapshot[1]?.kind).toMatchObject({ currentValue: undefined })
  })
})
