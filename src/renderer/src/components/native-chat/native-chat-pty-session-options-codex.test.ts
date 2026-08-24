import { describe, expect, it, vi } from 'vitest'
import { createNativeChatPtySessionOptions } from './native-chat-pty-session-options'

describe('Codex App Server session options', () => {
  it('applies model and effort directly when the live bridge is available', async () => {
    const dispatch = vi.fn().mockResolvedValue({
      outcome: 'applied',
      appliedValues: { model: 'gpt-5.6-sol', effort: 'high' }
    })
    const surface = createNativeChatPtySessionOptions({
      agent: 'codex',
      scopeKey: 'pty-bridge',
      initialModels: [
        {
          id: 'gpt-5.6-sol',
          label: 'GPT-5.6 Sol',
          isDefault: true,
          options: [
            {
              id: 'effort',
              label: 'Reasoning effort',
              category: 'thought_level',
              kind: {
                type: 'select',
                choices: [
                  { value: 'medium', label: 'Medium' },
                  { value: 'high', label: 'High' }
                ],
                defaultValue: 'high'
              },
              apply: { midSession: { kind: 'agent-picker', command: '/model' } }
            }
          ]
        }
      ],
      mode: 'live',
      directCodexSessionOptions: true,
      dispatchCommand: dispatch
    })!

    expect(surface.getSnapshot()[0].action).toBeUndefined()
    await surface.setOption('model', 'gpt-5.6-sol')

    expect(dispatch).toHaveBeenCalledWith('', {
      codexSessionOptions: { model: 'gpt-5.6-sol', effort: 'high' }
    })
    expect(surface.getSnapshot()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'model',
          valueSource: 'dispatched',
          kind: expect.objectContaining({ currentValue: 'gpt-5.6-sol' })
        }),
        expect.objectContaining({
          id: 'effort',
          valueSource: 'dispatched',
          kind: expect.objectContaining({ currentValue: 'high' })
        })
      ])
    )
  })

  it('persists effort under the effective model returned by Codex', async () => {
    const persistSelection = vi.fn()
    const surface = createNativeChatPtySessionOptions({
      agent: 'codex',
      scopeKey: 'pty-effective-model',
      initialModels: ['gpt-5.6-sol', 'gpt-5.6-terra'].map((id) => ({
        id,
        label: id,
        options: [
          {
            id: 'effort',
            label: 'Reasoning effort',
            category: 'thought_level' as const,
            kind: {
              type: 'select' as const,
              choices: [{ value: 'high', label: 'High' }],
              defaultValue: 'high'
            },
            apply: { midSession: { kind: 'agent-picker' as const, command: '/model' } }
          }
        ]
      })),
      mode: 'live',
      directCodexSessionOptions: true,
      dispatchCommand: vi.fn().mockResolvedValue({
        outcome: 'applied',
        appliedValues: { model: 'gpt-5.6-terra', effort: 'high' }
      }),
      persistSelection
    })!

    await surface.setOption('model', 'gpt-5.6-sol')

    expect(persistSelection).toHaveBeenCalledWith(
      expect.objectContaining({ modelId: 'gpt-5.6-terra', optionId: 'effort', value: 'high' })
    )
  })
})
