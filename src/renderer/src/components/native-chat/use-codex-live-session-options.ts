import { useEffect, useMemo, useState } from 'react'
import type { AgentType } from '../../../../shared/agent-status-types'
import type { CatalogModel } from '../../../../shared/agent-session-option-catalog'
import type {
  CodexLiveSessionModel,
  CodexLiveSessionOptionsSnapshot
} from '../../../../shared/codex-live-session-options'
import type { NativeChatSessionOptionDispatchCommand } from './native-chat-session-option-command-dispatch'

function codexLiveModelsToCatalogModels(models: CodexLiveSessionModel[]): CatalogModel[] {
  return models.map((model) => ({
    id: model.id,
    label: model.label,
    ...(model.description ? { description: model.description } : {}),
    ...(model.isDefault ? { isDefault: true } : {}),
    options:
      model.efforts.length === 0
        ? []
        : [
            {
              id: 'effort',
              label: 'Reasoning effort',
              category: 'thought_level',
              kind: {
                type: 'select',
                choices: model.efforts.map((effort) => ({
                  value: effort.id,
                  label: effort.label,
                  ...(effort.description ? { description: effort.description } : {})
                })),
                defaultValue: model.defaultEffort
              },
              apply: { midSession: { kind: 'agent-picker', command: '/model' } }
            }
          ]
  }))
}

export function useCodexLiveSessionOptions(args: {
  agent: AgentType
  targetPtyId: string | null
  dispatchCommand: NativeChatSessionOptionDispatchCommand
}): {
  snapshot: CodexLiveSessionOptionsSnapshot
  setSnapshot: React.Dispatch<React.SetStateAction<CodexLiveSessionOptionsSnapshot>>
  models: CatalogModel[] | null
  reportedValues: { model: string; effort?: string } | null
  dispatchCommand: NativeChatSessionOptionDispatchCommand
} {
  const { agent, targetPtyId, dispatchCommand: fallbackDispatchCommand } = args
  const [snapshot, setSnapshot] = useState<CodexLiveSessionOptionsSnapshot>({ available: false })
  useEffect(() => {
    if (agent !== 'codex' || !targetPtyId || !window.api?.pty?.getCodexSessionOptions) {
      setSnapshot((current) => (current.available ? { available: false } : current))
      return
    }
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | null = null
    const refresh = async (): Promise<void> => {
      try {
        const next = await window.api.pty.getCodexSessionOptions(targetPtyId)
        if (!cancelled) {
          setSnapshot(next)
          timer = setTimeout(refresh, next.available ? 5_000 : 1_500)
        }
      } catch {
        if (!cancelled) {
          setSnapshot({ available: false })
          timer = setTimeout(refresh, 1_500)
        }
      }
    }
    void refresh()
    return () => {
      cancelled = true
      if (timer) {
        clearTimeout(timer)
      }
    }
  }, [agent, targetPtyId])

  const dispatchCommand = useMemo<NativeChatSessionOptionDispatchCommand>(
    () => async (command, options) => {
      if (
        agent === 'codex' &&
        targetPtyId &&
        options?.codexSessionOptions &&
        window.api?.pty?.setCodexSessionOptions
      ) {
        const result = await window.api.pty.setCodexSessionOptions(
          targetPtyId,
          options.codexSessionOptions
        )
        if (result.outcome !== 'applied') {
          setSnapshot({ available: false })
          return { outcome: 'unknown' }
        }
        const appliedValues = {
          model: result.model ?? options.codexSessionOptions.model,
          effort: result.effort ?? options.codexSessionOptions.effort
        }
        setSnapshot((current) => {
          if (!current.available) {
            return current
          }
          const model = appliedValues.model ?? current.current?.model ?? current.models[0]?.id
          return model
            ? { ...current, current: { ...current.current, ...appliedValues, model } }
            : current
        })
        return { outcome: 'applied', appliedValues }
      }
      return fallbackDispatchCommand(command, options)
    },
    [agent, fallbackDispatchCommand, targetPtyId]
  )
  const models = useMemo(
    () =>
      agent === 'codex' && snapshot.available
        ? codexLiveModelsToCatalogModels(snapshot.models)
        : null,
    [agent, snapshot]
  )
  const reportedValues = useMemo(
    () =>
      agent === 'codex' && snapshot.available && snapshot.current
        ? {
            model: snapshot.current.model,
            ...(snapshot.current.effort ? { effort: snapshot.current.effort } : {})
          }
        : null,
    [agent, snapshot]
  )
  return { snapshot, setSnapshot, models, reportedValues, dispatchCommand }
}
