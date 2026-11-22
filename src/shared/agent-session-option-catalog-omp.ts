import { hasFlag } from './agent-cli-flag-detection'
import { removeAgentArgOption } from './agent-session-option-agent-args'
import type {
  AgentSessionOptionCatalog,
  CatalogModel,
  CatalogOption
} from './agent-session-option-catalog-types'
import {
  OMP_DEFAULT_MODEL_ID,
  OMP_MODEL_LIST_ARGS,
  parseOmpModelList
} from './omp-model-list-probe'

const OMP_THINKING_LABELS: Record<string, string> = {
  off: 'Off',
  minimal: 'Minimal',
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  xhigh: 'Extra high',
  max: 'Max',
  auto: 'Auto'
}

export function createOmpCatalogOptions(thinkingLevelIds: readonly string[]): CatalogOption[] {
  const choices = thinkingLevelIds
    .filter((id) => id in OMP_THINKING_LABELS)
    .map((id) => ({ value: id, label: OMP_THINKING_LABELS[id]! }))
  if (choices.length === 0) {
    return []
  }
  return [
    {
      id: 'effort',
      label: 'Reasoning effort',
      category: 'thought_level',
      kind: {
        type: 'select',
        choices,
        defaultValue: choices.some(({ value }) => value === 'high') ? 'high' : choices[0]!.value
      },
      apply: {
        launchArgs: (value) => ['--thinking', String(value)],
        agentArgsOverride: (tokens) => hasFlag(tokens, ['--thinking']),
        removeAgentArgs: (tokens) => removeAgentArgOption(tokens, ['--thinking']),
        midSession: { kind: 'command', build: (value) => `/mcode-thinking ${String(value)}` }
      }
    }
  ]
}

function parseOmpCatalogModels(stdout: string): CatalogModel[] {
  return parseOmpModelList(stdout).map((model) => ({
    id: model.id,
    label: model.label,
    ...(model.description ? { description: model.description } : {}),
    ...(model.modelMetadata ? { modelMetadata: { ...model.modelMetadata } } : {}),
    options: createOmpCatalogOptions(model.thinkingLevels?.map(({ id }) => id) ?? [])
  }))
}

const OMP_OPENAI_THINKING = createOmpCatalogOptions([
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max'
])

export const OMP_SESSION_OPTION_CATALOG: AgentSessionOptionCatalog = {
  models: [
    { id: OMP_DEFAULT_MODEL_ID, label: 'GPT-5.6-Sol', options: OMP_OPENAI_THINKING },
    {
      id: 'openai-codex/gpt-5.6-terra',
      label: 'GPT-5.6-Terra',
      options: OMP_OPENAI_THINKING
    },
    {
      id: 'openai-codex/gpt-5.6-luna',
      label: 'GPT-5.6-Luna',
      options: OMP_OPENAI_THINKING
    }
  ],
  modelApply: {
    launchArgs: (value) => ['--model', String(value)],
    agentArgsOverride: (tokens) => hasFlag(tokens, ['--model']),
    removeAgentArgs: (tokens) => removeAgentArgOption(tokens, ['--model']),
    midSession: { kind: 'command', build: (value) => `/mcode-model ${String(value)}` }
  },
  unknownModelOptions: OMP_OPENAI_THINKING,
  discoveredModelsAreAuthoritative: true,
  listModels: { command: `omp ${OMP_MODEL_LIST_ARGS.join(' ')}`, parse: parseOmpCatalogModels }
}
