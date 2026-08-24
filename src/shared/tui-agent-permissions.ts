import {
  applyAgentRunProfile,
  BYPASS_TUI_AGENT_ARGS,
  BYPASS_TUI_AGENT_ENV,
  resolveTuiAgentRunProfile,
  type AgentInteractionMode,
  type AgentPermissionPreset
} from './agent-run-profile'
import { TUI_AGENT_CONFIG } from './tui-agent-config'
import type { TuiAgent } from './tui-agent'

export type AgentPermissionMode = AgentPermissionPreset | 'mixed'

/** Backward-compatible export names used by launch-default migration code. */
export const YOLO_TUI_AGENT_ARGS = BYPASS_TUI_AGENT_ARGS
export const YOLO_TUI_AGENT_ENV = BYPASS_TUI_AGENT_ENV

const PERMISSION_AGENT_IDS = Object.keys(TUI_AGENT_CONFIG).filter(
  (agent): agent is TuiAgent => agent in YOLO_TUI_AGENT_ARGS || agent in YOLO_TUI_AGENT_ENV
)

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

function combinePermissionModes(modes: AgentPermissionMode[]): AgentPermissionMode {
  const concrete = new Set(modes.filter((mode): mode is AgentPermissionPreset => mode !== 'mixed'))
  return modes.includes('mixed') || concrete.size > 1
    ? 'mixed'
    : (concrete.values().next().value ?? 'ask')
}

export function resolveTuiAgentPermissionMode(args: {
  agent: TuiAgent
  agentArgs?: string | null
  agentEnv?: Record<string, string> | null
}): AgentPermissionMode {
  const modes: AgentPermissionMode[] = []
  if (args.agent in YOLO_TUI_AGENT_ARGS) {
    const resolved = resolveTuiAgentRunProfile(args.agent, args.agentArgs)
    modes.push(resolved.mcodeOwned ? (resolved.permission ?? 'mixed') : 'mixed')
  }
  if (args.agent in YOLO_TUI_AGENT_ENV) {
    const bypassEnv = YOLO_TUI_AGENT_ENV[args.agent]
    modes.push(
      sameEnv(args.agentEnv, {}) ? 'ask' : sameEnv(args.agentEnv, bypassEnv) ? 'bypass' : 'mixed'
    )
  }
  return combinePermissionModes(modes)
}

export function resolveAgentPermissionModeSummary(args: {
  agentDefaultArgs?: Partial<Record<TuiAgent, string>> | null
  agentDefaultEnv?: Partial<Record<TuiAgent, Record<string, string>>> | null
}): AgentPermissionMode {
  return combinePermissionModes(
    PERMISSION_AGENT_IDS.map((agent) =>
      resolveTuiAgentPermissionMode({
        agent,
        agentArgs: args.agentDefaultArgs?.[agent],
        agentEnv: args.agentDefaultEnv?.[agent]
      })
    )
  )
}

export function applyAgentPermissionMode(args: {
  mode: Exclude<AgentPermissionMode, 'mixed'>
  interactionMode?: AgentInteractionMode
  agentDefaultArgs?: Partial<Record<TuiAgent, string>> | null
  agentDefaultEnv?: Partial<Record<TuiAgent, Record<string, string>>> | null
}): {
  agentDefaultArgs: Partial<Record<TuiAgent, string>>
  agentDefaultEnv: Partial<Record<TuiAgent, Record<string, string>>>
} {
  return applyAgentRunProfile({
    permission: args.mode,
    interactionMode: args.interactionMode ?? 'build',
    agentDefaultArgs: args.agentDefaultArgs,
    agentDefaultEnv: args.agentDefaultEnv
  })
}
