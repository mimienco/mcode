import {
  findCatalogModel,
  findCatalogOption,
  type AgentSessionOptionCatalog,
  type CatalogModel
} from '../../../../shared/agent-session-option-catalog'
import type { SessionOptionValue } from '../../../../shared/native-chat-session-options'
import type {
  NativeChatSessionOptionDispatchCommand,
  NativeChatSessionOptionDispatchResult
} from './native-chat-session-option-command-dispatch'

export async function dispatchCodexLiveSessionOption(args: {
  catalog: AgentSessionOptionCatalog
  models: CatalogModel[]
  optionId: string
  value: SessionOptionValue
  modelId: string | null
  currentEffort?: SessionOptionValue
  dispatchCommand: NativeChatSessionOptionDispatchCommand
}): Promise<NativeChatSessionOptionDispatchResult | void> {
  const targetModelId =
    args.optionId === 'model' && typeof args.value === 'string' ? args.value : args.modelId
  if (!targetModelId) {
    throw new Error('Choose a model before changing its reasoning effort.')
  }
  const effortOption = findCatalogOption(
    findCatalogModel({ ...args.catalog, models: args.models }, targetModelId),
    'effort'
  )
  const targetEffort =
    args.optionId === 'effort' && typeof args.value === 'string'
      ? args.value
      : effortOption?.kind.type === 'select' &&
          typeof args.currentEffort === 'string' &&
          effortOption.kind.choices.some(({ value }) => value === args.currentEffort)
        ? args.currentEffort
        : effortOption?.kind.type === 'select'
          ? effortOption.kind.defaultValue
          : undefined
  return args.dispatchCommand('', {
    codexSessionOptions: {
      model: targetModelId,
      ...(targetEffort ? { effort: targetEffort } : {})
    }
  })
}
