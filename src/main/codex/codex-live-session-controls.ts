import type { AgentInteractionMode, AgentPermissionPreset } from '../../shared/agent-run-profile'

export type CodexCollaborationModePreset = {
  interactionMode: AgentInteractionMode
  wireMode: 'plan' | 'default'
  model?: string
  reasoningEffort?: string
}

const PERMISSION_PROFILE_BY_PRESET: Record<AgentPermissionPreset, string> = {
  ask: ':read-only',
  'auto-edits': ':workspace',
  bypass: ':danger-full-access'
}

const PRESET_BY_PERMISSION_PROFILE = new Map(
  Object.entries(PERMISSION_PROFILE_BY_PRESET).map(([preset, profile]) => [
    profile,
    preset as AgentPermissionPreset
  ])
)

function recordsFromList(value: unknown): Record<string, unknown>[] {
  if (!value || typeof value !== 'object') {
    return []
  }
  const data = (value as { data?: unknown }).data
  return Array.isArray(data)
    ? data.filter(
        (entry): entry is Record<string, unknown> =>
          Boolean(entry) && typeof entry === 'object' && !Array.isArray(entry)
      )
    : []
}

export function parseCodexPermissionProfiles(value: unknown): AgentPermissionPreset[] {
  const allowed = new Set<AgentPermissionPreset>()
  for (const entry of recordsFromList(value)) {
    if (entry.allowed !== true || typeof entry.id !== 'string') {
      continue
    }
    const preset = PRESET_BY_PERMISSION_PROFILE.get(entry.id)
    if (preset) {
      allowed.add(preset)
    }
  }
  return (['ask', 'auto-edits', 'bypass'] as const).filter((preset) => allowed.has(preset))
}

function normalizedRequirementSet(value: unknown): Set<string> | null {
  if (!Array.isArray(value)) {
    return null
  }
  return new Set(
    value
      .filter((entry): entry is string => typeof entry === 'string')
      .map((entry) => entry.replace(/[-_]/g, '').toLowerCase())
  )
}

/** Applies the independent admin allowlists exposed by configRequirements/read.
 * permissionProfile/list already reports an `allowed` bit, but older/partial
 * app-server builds can omit policy detail from that catalog. */
export function filterCodexPermissionProfilesByRequirements(
  profiles: readonly AgentPermissionPreset[],
  value: unknown
): AgentPermissionPreset[] {
  const requirements =
    value && typeof value === 'object'
      ? (value as { requirements?: unknown }).requirements
      : undefined
  if (requirements === null || !requirements || typeof requirements !== 'object') {
    return [...profiles]
  }
  const record = requirements as {
    allowedApprovalPolicies?: unknown
    allowedSandboxModes?: unknown
  }
  const approvals = normalizedRequirementSet(record.allowedApprovalPolicies)
  const sandboxes = normalizedRequirementSet(record.allowedSandboxModes)
  const allowedByRequirements: Record<AgentPermissionPreset, boolean> = {
    ask: (!approvals || approvals.has('onrequest')) && (!sandboxes || sandboxes.has('readonly')),
    'auto-edits':
      (!approvals || approvals.has('onrequest')) && (!sandboxes || sandboxes.has('workspacewrite')),
    bypass:
      (!approvals || approvals.has('never')) && (!sandboxes || sandboxes.has('dangerfullaccess'))
  }
  return profiles.filter((profile) => allowedByRequirements[profile])
}

export function parseCodexCollaborationModes(value: unknown): CodexCollaborationModePreset[] {
  const result: CodexCollaborationModePreset[] = []
  for (const entry of recordsFromList(value)) {
    const wireMode = entry.mode === 'plan' ? 'plan' : entry.mode === 'default' ? 'default' : null
    if (!wireMode) {
      continue
    }
    const preset: CodexCollaborationModePreset = {
      interactionMode: wireMode === 'plan' ? 'plan' : 'build',
      wireMode,
      ...(typeof entry.model === 'string' && entry.model ? { model: entry.model } : {}),
      ...(typeof entry.reasoning_effort === 'string' && entry.reasoning_effort
        ? { reasoningEffort: entry.reasoning_effort }
        : {})
    }
    if (!result.some((candidate) => candidate.interactionMode === preset.interactionMode)) {
      result.push(preset)
    }
  }
  return result
}

export function parseCodexThreadRunProfile(value: unknown): {
  permission?: AgentPermissionPreset
  interactionMode?: AgentInteractionMode
} {
  if (!value || typeof value !== 'object') {
    return {}
  }
  const settings = value as {
    activePermissionProfile?: unknown
    permissions?: unknown
    sandboxPolicy?: unknown
    collaborationMode?: unknown
  }
  const profile = settings.activePermissionProfile ?? settings.permissions
  const profileId =
    typeof profile === 'string'
      ? profile
      : profile && typeof profile === 'object'
        ? (profile as { id?: unknown }).id
        : undefined
  const directPermission =
    typeof profileId === 'string' ? PRESET_BY_PERMISSION_PROFILE.get(profileId) : undefined
  const sandboxType =
    settings.sandboxPolicy && typeof settings.sandboxPolicy === 'object'
      ? (settings.sandboxPolicy as { type?: unknown }).type
      : undefined
  const legacyPermission: AgentPermissionPreset | undefined =
    sandboxType === 'readOnly'
      ? 'ask'
      : sandboxType === 'workspaceWrite'
        ? 'auto-edits'
        : sandboxType === 'dangerFullAccess'
          ? 'bypass'
          : undefined
  const permission = directPermission ?? legacyPermission
  const collaborationMode = settings.collaborationMode
  const rawMode =
    collaborationMode && typeof collaborationMode === 'object'
      ? (collaborationMode as { mode?: unknown }).mode
      : collaborationMode
  const interactionMode = rawMode === 'plan' ? 'plan' : rawMode === 'default' ? 'build' : undefined
  return {
    ...(permission ? { permission } : {}),
    ...(interactionMode ? { interactionMode } : {})
  }
}

export function buildCodexRunProfileSettingsUpdate(args: {
  permission?: AgentPermissionPreset
  interactionMode?: AgentInteractionMode
  currentModel: string
  currentEffort?: string
  collaborationModes: readonly CodexCollaborationModePreset[]
}): Record<string, unknown> | null {
  const update: Record<string, unknown> = {}
  if (args.permission) {
    update.permissions = PERMISSION_PROFILE_BY_PRESET[args.permission]
    update.approvalPolicy = args.permission === 'bypass' ? 'never' : 'on-request'
  }
  if (args.interactionMode) {
    const preset = args.collaborationModes.find(
      (candidate) => candidate.interactionMode === args.interactionMode
    )
    if (!preset) {
      return null
    }
    update.collaborationMode = {
      mode: preset.wireMode,
      settings: {
        model: preset.model ?? args.currentModel,
        ...((preset.reasoningEffort ?? args.currentEffort)
          ? { reasoning_effort: preset.reasoningEffort ?? args.currentEffort }
          : {}),
        developer_instructions: null
      }
    }
  }
  return update
}
