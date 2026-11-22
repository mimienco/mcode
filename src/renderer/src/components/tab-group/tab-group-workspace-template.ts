import type { TuiAgent } from '../../../../shared/tui-agent'

export type TabGroupWorkspaceTemplateId = 'research' | 'build' | 'review'

type WorkspaceTemplateSlot = 'agent' | 'browser' | 'markdown' | 'terminal'

const TEMPLATE_SLOTS: Record<
  TabGroupWorkspaceTemplateId,
  readonly [WorkspaceTemplateSlot, WorkspaceTemplateSlot]
> = {
  research: ['browser', 'agent'],
  build: ['agent', 'terminal'],
  review: ['markdown', 'agent']
}

type LaunchTabGroupWorkspaceTemplateArgs = {
  templateId: TabGroupWorkspaceTemplateId
  agent: TuiAgent
  worktreeId: string
  sourceGroupId: string
  createEmptySplitGroup: (
    worktreeId: string,
    sourceGroupId: string,
    direction: 'right'
  ) => string | null
  closeEmptyGroup: (worktreeId: string, groupId: string) => boolean
  openAgent: (agent: TuiAgent, groupId: string) => Promise<void>
  openBrowser: (groupId: string) => Promise<void>
  openMarkdown: (groupId: string) => Promise<void>
  openTerminal: (groupId: string) => Promise<void>
}

export async function launchTabGroupWorkspaceTemplate({
  templateId,
  agent,
  worktreeId,
  sourceGroupId,
  createEmptySplitGroup,
  closeEmptyGroup,
  openAgent,
  openBrowser,
  openMarkdown,
  openTerminal
}: LaunchTabGroupWorkspaceTemplateArgs): Promise<'launched' | 'split-unavailable'> {
  const splitGroupId = createEmptySplitGroup(worktreeId, sourceGroupId, 'right')
  if (!splitGroupId) {
    return 'split-unavailable'
  }

  const targetGroupIds = [sourceGroupId, splitGroupId] as const
  const slots = TEMPLATE_SLOTS[templateId]

  try {
    const openings = slots.map((slot, index) => {
      const targetGroupId = targetGroupIds[index]
      if (slot === 'agent') {
        return openAgent(agent, targetGroupId)
      }
      if (slot === 'browser') {
        return openBrowser(targetGroupId)
      }
      if (slot === 'markdown') {
        return openMarkdown(targetGroupId)
      }
      return openTerminal(targetGroupId)
    })
    const outcomes = await Promise.allSettled(openings)
    const failure = outcomes.find((outcome) => outcome.status === 'rejected')
    if (failure?.status === 'rejected') {
      throw failure.reason
    }
  } catch (error) {
    closeEmptyGroup(worktreeId, splitGroupId)
    throw error
  }

  return 'launched'
}
