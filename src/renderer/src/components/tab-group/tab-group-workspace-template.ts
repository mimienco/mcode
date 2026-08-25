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
    for (const [index, slot] of slots.entries()) {
      const targetGroupId = targetGroupIds[index]
      if (slot === 'agent') {
        await openAgent(agent, targetGroupId)
      } else if (slot === 'browser') {
        await openBrowser(targetGroupId)
      } else if (slot === 'markdown') {
        await openMarkdown(targetGroupId)
      } else {
        await openTerminal(targetGroupId)
      }
    }
  } catch (error) {
    closeEmptyGroup(worktreeId, splitGroupId)
    throw error
  }

  return 'launched'
}
