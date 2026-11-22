import { runQuickCommandInNewTab } from '@/lib/run-quick-command-in-new-tab'
import { useAppStore } from '@/store'
import type { Tab } from '../../../../shared/tab-types'
import { isAllowedClaudeUpdateCommand } from './native-chat-terminal-attention'

type UpdateTarget = { worktreeId: string; groupId: string }

export function resolveNativeChatUpdateTarget(
  tabsByWorktree: Record<string, readonly Tab[]>,
  terminalTabId: string
): UpdateTarget | null {
  for (const [worktreeId, tabs] of Object.entries(tabsByWorktree)) {
    const tab = tabs.find(
      (candidate) =>
        candidate.contentType === 'terminal' &&
        (candidate.entityId === terminalTabId || candidate.id === terminalTabId)
    )
    if (tab) {
      return { worktreeId, groupId: tab.groupId }
    }
  }
  return null
}

export function launchClaudeUpdateInNewTab(terminalTabId: string, command: string): boolean {
  if (!isAllowedClaudeUpdateCommand(command)) {
    return false
  }
  const target = resolveNativeChatUpdateTarget(
    useAppStore.getState().unifiedTabsByWorktree,
    terminalTabId
  )
  if (!target) {
    return false
  }
  return Boolean(
    runQuickCommandInNewTab({
      command: {
        id: 'native-chat-claude-update',
        label: 'Update Claude Code',
        action: 'terminal-command',
        command,
        appendEnter: true
      },
      worktreeId: target.worktreeId,
      groupId: target.groupId
    })
  )
}
