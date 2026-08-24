import { describe, expect, it, vi } from 'vitest'
import type { AgentSessionOptionCatalog } from '../../../../shared/agent-session-option-catalog'
import { dispatchCodexLiveSessionOption } from './native-chat-codex-session-option-apply'

const catalog: AgentSessionOptionCatalog = {
  models: [],
  modelApply: {}
}

describe('dispatchCodexLiveSessionOption', () => {
  it('preserves the current effort when the target model supports it', async () => {
    const dispatchCommand = vi.fn()

    await dispatchCodexLiveSessionOption({
      catalog,
      models: [
        {
          id: 'gpt-5.6-terra',
          label: 'GPT-5.6 Terra',
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
              apply: {}
            }
          ]
        }
      ],
      optionId: 'model',
      value: 'gpt-5.6-terra',
      modelId: 'gpt-5.6-sol',
      currentEffort: 'medium',
      dispatchCommand
    })

    expect(dispatchCommand).toHaveBeenCalledWith('', {
      codexSessionOptions: { model: 'gpt-5.6-terra', effort: 'medium' }
    })
  })

  it('uses the target default when its effort list rejects the current value', async () => {
    const dispatchCommand = vi.fn()

    await dispatchCodexLiveSessionOption({
      catalog,
      models: [
        {
          id: 'gpt-5.6-luna',
          label: 'GPT-5.6 Luna',
          options: [
            {
              id: 'effort',
              label: 'Reasoning effort',
              category: 'thought_level',
              kind: {
                type: 'select',
                choices: [{ value: 'medium', label: 'Medium' }],
                defaultValue: 'medium'
              },
              apply: {}
            }
          ]
        }
      ],
      optionId: 'model',
      value: 'gpt-5.6-luna',
      modelId: 'gpt-5.6-sol',
      currentEffort: 'ultra',
      dispatchCommand
    })

    expect(dispatchCommand).toHaveBeenCalledWith('', {
      codexSessionOptions: { model: 'gpt-5.6-luna', effort: 'medium' }
    })
  })
})
