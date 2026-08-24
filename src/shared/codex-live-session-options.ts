import type { AgentInteractionMode, AgentPermissionPreset } from './agent-run-profile'

export type CodexLiveSessionEffort = {
  id: string
  label: string
  description?: string
}

export type CodexLiveSessionModel = {
  id: string
  label: string
  description?: string
  efforts: CodexLiveSessionEffort[]
  defaultEffort: string
  isDefault?: boolean
}

export type CodexLiveSessionOptionsSnapshot =
  | { available: false }
  | {
      available: true
      models: CodexLiveSessionModel[]
      controls: {
        permissions: AgentPermissionPreset[]
        interactionModes: AgentInteractionMode[]
      }
      current?: {
        model: string
        effort?: string
        permission?: AgentPermissionPreset
        interactionMode?: AgentInteractionMode
      }
    }

export type CodexLiveSessionOptionsUpdate = {
  model?: string
  effort?: string
  permission?: AgentPermissionPreset
  interactionMode?: AgentInteractionMode
}

export type CodexLiveSessionOptionsUpdateResult =
  | {
      outcome: 'applied'
      model?: string
      effort?: string
      permission?: AgentPermissionPreset
      interactionMode?: AgentInteractionMode
    }
  | { outcome: 'unavailable' }
