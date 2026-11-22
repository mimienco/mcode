import type { CommitMessageModel } from './commit-message-agent-spec'
import { assertJsonTextStructureWithinLimits } from './json-text-structure-limit'
import { labelFromModelId } from './model-id-label'

export const OMP_MODEL_LIST_ARGS = ['models', '--json']
export const OMP_DEFAULT_MODEL_ID = 'openai-codex/gpt-5.6-sol'

const OMP_MODEL_JSON_STRUCTURE_LIMITS = {
  structuralTokens: 64 * 1024,
  nestingDepth: 16
} as const

const THINKING_LABELS: Record<string, string> = {
  off: 'Off',
  minimal: 'Minimal',
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  xhigh: 'Extra High',
  max: 'Max',
  auto: 'Auto'
}

const OMP_PROVIDER_LABELS: Record<string, string> = {
  bai: 'B.AI',
  'google-antigravity': 'Google Antigravity',
  'openai-codex': 'OpenAI Codex',
  openrouter: 'OpenRouter',
  zenmux: 'ZenMux'
}

function finiteNonnegative(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : undefined
}

/** Parse OMP's authenticated `models --json` surface. The provider-qualified
 * selector is the only unambiguous value accepted across all providers. */
export function parseOmpModelList(stdout: string): CommitMessageModel[] {
  try {
    assertJsonTextStructureWithinLimits(stdout, OMP_MODEL_JSON_STRUCTURE_LIMITS)
    const parsed = JSON.parse(stdout) as {
      models?: {
        provider?: unknown
        selector?: unknown
        name?: unknown
        contextWindow?: unknown
        maxTokens?: unknown
        thinking?: unknown
        cost?: { input?: unknown; output?: unknown }
      }[]
    }
    const seen = new Set<string>()
    const models: CommitMessageModel[] = []
    for (const entry of parsed.models ?? []) {
      const id = typeof entry.selector === 'string' ? entry.selector.trim() : ''
      if (!id || seen.has(id)) {
        continue
      }
      seen.add(id)
      const thinkingIds = Array.isArray(entry.thinking)
        ? entry.thinking.filter(
            (value): value is string => typeof value === 'string' && value in THINKING_LABELS
          )
        : []
      const providerId =
        typeof entry.provider === 'string' && entry.provider.trim()
          ? entry.provider.trim()
          : id.split('/')[0]!
      const contextWindowTokens = finiteNonnegative(entry.contextWindow)
      const maxOutputTokens = finiteNonnegative(entry.maxTokens)
      const inputPerMillion = finiteNonnegative(entry.cost?.input)
      const outputPerMillion = finiteNonnegative(entry.cost?.output)
      models.push({
        id,
        label:
          typeof entry.name === 'string' && entry.name.trim()
            ? entry.name.trim()
            : labelFromModelId(id),
        modelMetadata: {
          availability: 'available',
          providerId,
          providerLabel: OMP_PROVIDER_LABELS[providerId] ?? labelFromModelId(providerId),
          ...(contextWindowTokens === undefined ? {} : { contextWindowTokens }),
          ...(maxOutputTokens === undefined ? {} : { maxOutputTokens }),
          ...(inputPerMillion === undefined && outputPerMillion === undefined
            ? {}
            : {
                pricing: {
                  ...(inputPerMillion === undefined ? {} : { inputPerMillion }),
                  ...(outputPerMillion === undefined ? {} : { outputPerMillion })
                }
              })
        },
        ...(thinkingIds.length > 0
          ? {
              thinkingLevels: thinkingIds.map((thinkingId) => ({
                id: thinkingId,
                label: THINKING_LABELS[thinkingId]!
              })),
              defaultThinkingLevel: thinkingIds.includes('high') ? 'high' : thinkingIds[0]
            }
          : {})
      })
    }
    return models
  } catch {
    return []
  }
}
