import type { Worktree } from '../../../../shared/worktree/types'
import { getRepoIdFromWorktreeId } from '../../../../shared/worktree/id'

export type AgentPersona = {
  name?: string
  color?: string
}

/** Why: palette kept small and AA-readable on both sidebar surfaces. */
export const AGENT_PERSONA_COLORS = [
  '#e05d44',
  '#d98324',
  '#b5a642',
  '#3f9142',
  '#3f7fbf',
  '#7a5fb5',
  '#bf5f9e',
  '#4fa3a5'
] as const

export function findWorktreeById(
  worktreesByRepo: Record<string, Worktree[]> | undefined,
  worktreeId: string | undefined
): Worktree | undefined {
  // Why: retained/synthetic rows and bare test harnesses may carry no worktree id.
  if (!worktreesByRepo || !worktreeId) {
    return undefined
  }
  return worktreesByRepo[getRepoIdFromWorktreeId(worktreeId)]?.find(
    (entry) => entry.id === worktreeId
  )
}

export function getWorktreeAgentPersona(
  worktreesByRepo: Record<string, Worktree[]> | undefined,
  worktreeId: string | undefined,
  agentType: string
): AgentPersona | undefined {
  const worktree = findWorktreeById(worktreesByRepo, worktreeId)
  return worktree?.personas?.[agentType]
}

export function nextAgentPersonaColor(current: string | undefined): string {
  const index = AGENT_PERSONA_COLORS.indexOf(current as (typeof AGENT_PERSONA_COLORS)[number])
  return AGENT_PERSONA_COLORS[(index + 1) % AGENT_PERSONA_COLORS.length]
}
