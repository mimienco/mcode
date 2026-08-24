import type { AgentType } from '../../../../shared/agent-status-types'
import {
  getAgentRunProfileCapabilities,
  normalizeAgentInteractionMode,
  normalizeAgentPermissionPreset,
  type AgentInteractionMode,
  type AgentPermissionPreset,
  type AgentRunProfileSupport
} from '../../../../shared/agent-run-profile'
import type { CodexLiveSessionOptionsSnapshot } from '../../../../shared/codex-live-session-options'
import type {
  SessionOptionDescriptor,
  SessionOptionSelectChoice,
  SessionOptionValue
} from '../../../../shared/native-chat-session-options'

const PERMISSION_CHOICES: readonly Omit<
  SessionOptionSelectChoice,
  'disabled' | 'disabledReason'
>[] = [
  {
    value: 'ask',
    label: 'Ask approval',
    description: 'Ask before edits and untrusted commands.'
  },
  {
    value: 'auto-edits',
    label: 'Auto-accept edits',
    description: 'Allow workspace edits; keep wider actions behind provider checks.'
  },
  {
    value: 'bypass',
    label: 'Bypass permissions',
    description: 'Remove provider prompts and sandbox restrictions where supported.'
  }
]

const INTERACTION_CHOICES: readonly Omit<
  SessionOptionSelectChoice,
  'disabled' | 'disabledReason'
>[] = [
  { value: 'plan', label: 'Plan', description: 'Research and prepare a plan before editing.' },
  { value: 'build', label: 'Build', description: 'Use normal implementation behavior.' }
]

function choiceWithSupport(
  choice: Omit<SessionOptionSelectChoice, 'disabled' | 'disabledReason'>,
  support: AgentRunProfileSupport
): SessionOptionSelectChoice {
  if (support.kind === 'unsupported') {
    return { ...choice, disabled: true, disabledReason: support.reason }
  }
  if (support.kind === 'interactive-fallback') {
    return {
      ...choice,
      disabled: true,
      disabledReason: `${support.actionLabel}; this session cannot report the result.`
    }
  }
  return { ...choice }
}

export function buildNativeChatRunProfileSnapshot(args: {
  agent: AgentType
  permission: AgentPermissionPreset
  interactionMode: AgentInteractionMode
  codexLiveSnapshot: CodexLiveSessionOptionsSnapshot
  customLaunchOverride?: boolean
}): SessionOptionDescriptor[] {
  const staticCapabilities = getAgentRunProfileCapabilities(args.agent)
  const liveSnapshot =
    args.agent === 'codex' && args.codexLiveSnapshot.available ? args.codexLiveSnapshot : null
  const live = liveSnapshot !== null
  const launchOverride = !live && args.customLaunchOverride === true
  const livePermissions = liveSnapshot ? new Set(liveSnapshot.controls.permissions) : null
  const liveModes = liveSnapshot ? new Set(liveSnapshot.controls.interactionModes) : null
  const permissionChoices = PERMISSION_CHOICES.map((choice) =>
    launchOverride
      ? {
          ...choice,
          disabled: true,
          disabledReason: 'Custom launch arguments override the general setting.'
        }
      : livePermissions
        ? livePermissions.has(choice.value as AgentPermissionPreset)
          ? { ...choice }
          : { ...choice, disabled: true, disabledReason: 'Blocked or unavailable in this session.' }
        : choiceWithSupport(
            choice,
            staticCapabilities.permissions[choice.value as AgentPermissionPreset]
          )
  )
  const interactionChoices = INTERACTION_CHOICES.map((choice) =>
    launchOverride
      ? {
          ...choice,
          disabled: true,
          disabledReason: 'Custom launch arguments override the general setting.'
        }
      : liveModes
        ? liveModes.has(choice.value as AgentInteractionMode)
          ? { ...choice }
          : { ...choice, disabled: true, disabledReason: 'Unavailable in this session.' }
        : choiceWithSupport(
            choice,
            staticCapabilities.interactionModes[choice.value as AgentInteractionMode]
          )
  )
  const current = liveSnapshot?.current
  const applyScope = live ? ('current-session' as const) : ('next-session' as const)
  const valueSource = live ? ('reported' as const) : ('default' as const)
  const requestedPermissionSupport = staticCapabilities.permissions[args.permission]
  const displayedPermission =
    !live && requestedPermissionSupport.kind === 'unsupported' ? 'ask' : args.permission
  const requestedInteractionSupport = staticCapabilities.interactionModes[args.interactionMode]
  const displayedInteractionMode =
    !live &&
    (requestedInteractionSupport.kind === 'unsupported' ||
      requestedInteractionSupport.kind === 'interactive-fallback')
      ? 'build'
      : args.interactionMode

  return [
    {
      id: 'permission',
      label: 'Permissions',
      description: live
        ? 'Applies to this task now.'
        : launchOverride
          ? 'Custom provider launch arguments are active; change them in Settings.'
          : 'Saved for new sessions; restart this agent to apply it here.',
      category: 'permission',
      kind: {
        type: 'select',
        currentValue: live ? current?.permission : launchOverride ? undefined : displayedPermission,
        choices: permissionChoices
      },
      valueSource,
      applyScope,
      settable: permissionChoices.some((choice) => !choice.disabled)
    },
    {
      id: 'interaction-mode',
      label: 'Work mode',
      description: live
        ? 'Applies to this task now.'
        : launchOverride
          ? 'Custom provider launch arguments are active; change them in Settings.'
          : 'Saved for new sessions; restart this agent to apply it here.',
      category: 'interaction_mode',
      kind: {
        type: 'select',
        currentValue: live
          ? current?.interactionMode
          : launchOverride
            ? undefined
            : displayedInteractionMode,
        choices: interactionChoices
      },
      valueSource,
      applyScope,
      settable: interactionChoices.some((choice) => !choice.disabled)
    }
  ]
}

export function isAgentPermissionPreset(value: SessionOptionValue): value is AgentPermissionPreset {
  return normalizeAgentPermissionPreset(value) !== undefined
}

export function isAgentInteractionMode(value: SessionOptionValue): value is AgentInteractionMode {
  return normalizeAgentInteractionMode(value) !== undefined
}
