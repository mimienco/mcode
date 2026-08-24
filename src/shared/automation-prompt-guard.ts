import type { Automation } from './automations-types'

export const AUTOMATION_PUSH_GUARD_SENTINEL = '[MCode automation safety rail]'

// Why prompt-level: scheduled runs execute through the agent CLIs themselves, so
// the only enforcement point MCode owns before WP-governance lands is the
// instruction boundary. Prepended (not appended) so long agent transcripts or
// prompt truncation can never drop the guard below the user's task text.
export function buildAutomationRunPrompt(
  automation: Pick<Automation, 'prompt' | 'autoPushEnabled'>
): string {
  if (automation.autoPushEnabled === true) {
    return automation.prompt
  }
  return [
    `${AUTOMATION_PUSH_GUARD_SENTINEL} Work strictly inside this workspace/worktree.`,
    'Do NOT run `git push`, do NOT create, comment on, or merge pull requests, and do NOT modify git remotes or credentials.',
    'Commit locally and leave every change in the worktree for human review.',
    '',
    automation.prompt
  ].join('\n')
}
