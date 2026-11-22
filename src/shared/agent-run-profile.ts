import type { AgentType } from './agent-status-types'
import { TUI_AGENT_CONFIG } from './tui-agent-config'
import type { TuiAgent } from './tui-agent'

export type AgentPermissionPreset = 'ask' | 'auto-edits' | 'bypass'
export type AgentInteractionMode = 'plan' | 'build'

export function normalizeAgentPermissionPreset(value: unknown): AgentPermissionPreset | undefined {
  return value === 'ask' || value === 'auto-edits' || value === 'bypass' ? value : undefined
}

export function normalizeAgentInteractionMode(value: unknown): AgentInteractionMode | undefined {
  return value === 'plan' || value === 'build' ? value : undefined
}

export type AgentRunProfileSupport =
  | { kind: 'live' }
  | { kind: 'launch-only' }
  | { kind: 'interactive-fallback'; actionLabel: string }
  | { kind: 'unsupported'; reason: string }

export type AgentRunProfileCapabilities = {
  permissions: Record<AgentPermissionPreset, AgentRunProfileSupport>
  interactionModes: Record<AgentInteractionMode, AgentRunProfileSupport>
}

const LAUNCH_ONLY: AgentRunProfileSupport = { kind: 'launch-only' }
const UNSUPPORTED_AUTO_EDITS: AgentRunProfileSupport = {
  kind: 'unsupported',
  reason: 'This provider has no session-scoped auto-edits preset.'
}
const UNSUPPORTED_PLAN: AgentRunProfileSupport = {
  kind: 'unsupported',
  reason: 'This provider has no verified Plan/Build launch mode.'
}

/** Existing MCode bypass launch coverage. Kept here as the canonical mapping so
 * Settings, launch planning, and native Chat cannot drift apart. */
export const BYPASS_TUI_AGENT_ARGS: Partial<Record<TuiAgent, string>> = {
  claude: '--dangerously-skip-permissions',
  'claude-agent-teams': '--dangerously-skip-permissions',
  openclaude: '--dangerously-skip-permissions',
  codex: '--dangerously-bypass-approvals-and-sandbox',
  gemini: '--approval-mode yolo',
  antigravity: '--dangerously-skip-permissions',
  aider: '--yes-always',
  amp: '--dangerously-allow-all',
  kiro: '--trust-all-tools',
  crush: '--yolo',
  autohand: '--unrestricted',
  cline: '--auto-approve true',
  'command-code': '--yolo',
  continue: '--allow "*"',
  cursor: '--yolo',
  kimi: '--yolo',
  'mistral-vibe': '--agent auto-approve',
  'qwen-code': '--approval-mode yolo',
  rovo: '--yolo',
  hermes: '--yolo',
  copilot: '--yolo',
  grok: '--permission-mode bypassPermissions',
  omp: '--approval-mode yolo',
  devin: '--permission-mode bypass',
  ante: '--yolo',
  trae: '--yolo'
}

export const BYPASS_TUI_AGENT_ENV: Partial<Record<TuiAgent, Record<string, string>>> = {
  goose: { GOOSE_MODE: 'auto' }
}

const ASK_ARGS: Partial<Record<TuiAgent, string>> = {
  claude: '--permission-mode default',
  codex: '--sandbox read-only --ask-for-approval on-request',
  gemini: '--approval-mode default',
  grok: '--permission-mode default',
  omp: '--approval-mode always-ask'
}

const AUTO_EDIT_ARGS: Partial<Record<TuiAgent, string>> = {
  claude: '--permission-mode acceptEdits',
  codex: '--sandbox workspace-write --ask-for-approval on-request',
  gemini: '--approval-mode auto_edit',
  grok: '--permission-mode acceptEdits',
  omp: '--approval-mode write'
}

const COMBINED_PLAN_ARGS: Partial<Record<TuiAgent, string>> = {
  claude: '--permission-mode plan',
  gemini: '--approval-mode plan'
}

const INDEPENDENT_PLAN_ARGS: Partial<Record<TuiAgent, string>> = {
  cursor: '--mode plan'
}

const RUN_PROFILE_AGENT_IDS = Object.keys(TUI_AGENT_CONFIG).filter(
  (agent): agent is TuiAgent =>
    agent in BYPASS_TUI_AGENT_ARGS ||
    agent in BYPASS_TUI_AGENT_ENV ||
    agent in ASK_ARGS ||
    agent in AUTO_EDIT_ARGS ||
    agent in COMBINED_PLAN_ARGS ||
    agent in INDEPENDENT_PLAN_ARGS
)

function normalizeArgs(value: string | null | undefined): string {
  return value?.trim().replace(/\s+/g, ' ') ?? ''
}

function permissionArgs(agent: TuiAgent, permission: AgentPermissionPreset): string {
  if (permission === 'bypass') {
    return BYPASS_TUI_AGENT_ARGS[agent] ?? ''
  }
  if (permission === 'auto-edits') {
    // Unsupported auto-edits intentionally degrades to Ask, never Bypass.
    return AUTO_EDIT_ARGS[agent] ?? ASK_ARGS[agent] ?? ''
  }
  return ASK_ARGS[agent] ?? ''
}

export function buildAgentRunProfileArgs(args: {
  agent: TuiAgent
  permission: AgentPermissionPreset
  interactionMode: AgentInteractionMode
}): string {
  const combinedPlan = COMBINED_PLAN_ARGS[args.agent]
  if (args.interactionMode === 'plan' && combinedPlan) {
    return combinedPlan
  }
  const permission = permissionArgs(args.agent, args.permission)
  const independentPlan =
    args.interactionMode === 'plan' ? (INDEPENDENT_PLAN_ARGS[args.agent] ?? '') : ''
  return [permission, independentPlan].filter(Boolean).join(' ')
}

function sameEnv(
  left: Record<string, string> | null | undefined,
  right: Record<string, string> | null | undefined
): boolean {
  const leftEntries = Object.entries(left ?? {})
  const rightEntries = Object.entries(right ?? {})
  return (
    leftEntries.length === rightEntries.length &&
    leftEntries.every(([name, value]) => right?.[name] === value)
  )
}

function mcodeOwnedArgs(agent: TuiAgent, value: string): boolean {
  if (!value) {
    return true
  }
  for (const permission of ['ask', 'auto-edits', 'bypass'] as const) {
    for (const interactionMode of ['build', 'plan'] as const) {
      if (
        normalizeArgs(buildAgentRunProfileArgs({ agent, permission, interactionMode })) === value
      ) {
        return true
      }
    }
  }
  // Legacy aliases written by earlier MCode versions remain replaceable.
  return (
    value === normalizeArgs(BYPASS_TUI_AGENT_ARGS[agent]) ||
    (agent === 'gemini' && value === '--yolo')
  )
}

export function applyAgentRunProfile(args: {
  permission: AgentPermissionPreset
  interactionMode: AgentInteractionMode
  agentDefaultArgs?: Partial<Record<TuiAgent, string>> | null
  agentDefaultEnv?: Partial<Record<TuiAgent, Record<string, string>>> | null
}): {
  agentDefaultArgs: Partial<Record<TuiAgent, string>>
  agentDefaultEnv: Partial<Record<TuiAgent, Record<string, string>>>
} {
  const nextArgs = { ...args.agentDefaultArgs }
  const nextEnv = { ...args.agentDefaultEnv }

  for (const agent of RUN_PROFILE_AGENT_IDS) {
    const currentArgs = normalizeArgs(nextArgs[agent])
    if (mcodeOwnedArgs(agent, currentArgs)) {
      nextArgs[agent] = buildAgentRunProfileArgs({
        agent,
        permission: args.permission,
        interactionMode: args.interactionMode
      })
    }

    if (agent in BYPASS_TUI_AGENT_ENV) {
      const bypassEnv = BYPASS_TUI_AGENT_ENV[agent]
      const currentEnv = nextEnv[agent]
      if (sameEnv(currentEnv, {}) || sameEnv(currentEnv, bypassEnv)) {
        nextEnv[agent] = args.permission === 'bypass' ? { ...bypassEnv } : {}
      }
    }
  }

  return { agentDefaultArgs: nextArgs, agentDefaultEnv: nextEnv }
}

export function resolveTuiAgentRunProfile(
  agent: TuiAgent,
  agentArgs?: string | null
): {
  permission?: AgentPermissionPreset
  interactionMode?: AgentInteractionMode
  mcodeOwned: boolean
} {
  const value = normalizeArgs(agentArgs)
  if (!mcodeOwnedArgs(agent, value)) {
    return { mcodeOwned: false }
  }
  const combinedPlan = COMBINED_PLAN_ARGS[agent]
  if (combinedPlan && value === normalizeArgs(combinedPlan)) {
    return { permission: undefined, interactionMode: 'plan', mcodeOwned: true }
  }
  const hasIndependentPlan = Boolean(INDEPENDENT_PLAN_ARGS[agent])
  for (const permission of ['ask', 'auto-edits', 'bypass'] as const) {
    if (
      hasIndependentPlan &&
      value ===
        normalizeArgs(buildAgentRunProfileArgs({ agent, permission, interactionMode: 'plan' }))
    ) {
      return { permission, interactionMode: 'plan', mcodeOwned: true }
    }
    if (
      value ===
      normalizeArgs(buildAgentRunProfileArgs({ agent, permission, interactionMode: 'build' }))
    ) {
      return { permission, interactionMode: 'build', mcodeOwned: true }
    }
  }
  return { permission: 'ask', interactionMode: 'build', mcodeOwned: true }
}

export function getAgentRunProfileCapabilities(agent: AgentType): AgentRunProfileCapabilities {
  const hasBypass = agent in BYPASS_TUI_AGENT_ARGS || agent in BYPASS_TUI_AGENT_ENV
  const hasAutoEdits = agent in AUTO_EDIT_ARGS
  const hasLaunchPlan = agent in COMBINED_PLAN_ARGS || agent in INDEPENDENT_PLAN_ARGS
  const hasInteractivePlan = agent === 'codex' || agent === 'grok' || agent === 'omp'
  const interactivePlanAction =
    agent === 'grok'
      ? 'Use /plan or Shift+Tab in the Grok terminal'
      : agent === 'omp'
        ? 'Use /plan in the OMP terminal'
        : 'Open Plan in Terminal'
  return {
    permissions: {
      ask: hasBypass ? LAUNCH_ONLY : { kind: 'unsupported', reason: 'No verified approval mode.' },
      'auto-edits': hasAutoEdits ? LAUNCH_ONLY : UNSUPPORTED_AUTO_EDITS,
      bypass: hasBypass ? LAUNCH_ONLY : { kind: 'unsupported', reason: 'No verified bypass mode.' }
    },
    interactionModes: {
      plan: hasLaunchPlan
        ? LAUNCH_ONLY
        : hasInteractivePlan
          ? { kind: 'interactive-fallback', actionLabel: interactivePlanAction }
          : UNSUPPORTED_PLAN,
      build: LAUNCH_ONLY
    }
  }
}
