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
      current?: { model: string; effort?: string }
    }

export type CodexLiveSessionOptionsUpdate = {
  model?: string
  effort?: string
}

export type CodexLiveSessionOptionsUpdateResult =
  | { outcome: 'applied'; model?: string; effort?: string }
  | { outcome: 'unavailable' }
