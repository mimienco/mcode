import { describe, expect, it } from 'vitest'
import { parseOmpModelList } from './omp-model-list-probe'

describe('parseOmpModelList', () => {
  it('keeps provider-qualified selectors and model-specific thinking levels', () => {
    expect(
      parseOmpModelList(
        JSON.stringify({
          models: [
            {
              provider: 'openai-codex',
              id: 'gpt-5.6-sol',
              selector: 'openai-codex/gpt-5.6-sol',
              name: 'GPT-5.6-Sol',
              contextWindow: 272000,
              maxTokens: 128000,
              thinking: ['low', 'high', 'xhigh', 'max'],
              cost: { input: 1.75, output: 14, cacheRead: 0.175, cacheWrite: 0 }
            },
            {
              provider: 'openrouter',
              id: 'fast',
              selector: 'openrouter/fast',
              name: 'Fast',
              contextWindow: 1000000,
              maxTokens: 64000,
              thinking: null,
              cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }
            }
          ]
        })
      )
    ).toEqual([
      {
        id: 'openai-codex/gpt-5.6-sol',
        label: 'GPT-5.6-Sol',
        thinkingLevels: [
          { id: 'low', label: 'Low' },
          { id: 'high', label: 'High' },
          { id: 'xhigh', label: 'Extra High' },
          { id: 'max', label: 'Max' }
        ],
        defaultThinkingLevel: 'high',
        modelMetadata: {
          availability: 'available',
          providerId: 'openai-codex',
          providerLabel: 'OpenAI Codex',
          contextWindowTokens: 272000,
          maxOutputTokens: 128000,
          pricing: { inputPerMillion: 1.75, outputPerMillion: 14 }
        }
      },
      {
        id: 'openrouter/fast',
        label: 'Fast',
        modelMetadata: {
          availability: 'available',
          providerId: 'openrouter',
          providerLabel: 'OpenRouter',
          contextWindowTokens: 1000000,
          maxOutputTokens: 64000,
          pricing: { inputPerMillion: 0, outputPerMillion: 0 }
        }
      }
    ])
  })

  it('rejects malformed output', () => {
    expect(parseOmpModelList('not json')).toEqual([])
    expect(parseOmpModelList('{"models":"wrong"}')).toEqual([])
  })
})
