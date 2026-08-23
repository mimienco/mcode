import type { FolderWorkspace } from './folder-workspace-types'
import type { ProjectGroup } from './project-group-types'
import { resolveRuntimePath } from './cross-platform-path'
import { parseWorkspaceKey } from './workspace-scope'

export const QUICK_CHATS_GROUP_NAME = 'Chats'
export const QUICK_CHAT_DEFAULT_NAME = 'New chat'
export const QUICK_CHATS_DIRECTORY_NAME = 'chats'

const SAFE_QUICK_CHAT_ID = /^[a-zA-Z0-9][a-zA-Z0-9_-]*$/

export function isQuickChatProjectGroup(
  group: ProjectGroup | null | undefined
): group is ProjectGroup & { purpose: 'quick-chats' } {
  return group?.purpose === 'quick-chats'
}

export function isQuickChatWorkspace(
  workspace: FolderWorkspace | null | undefined
): workspace is FolderWorkspace & { kind: 'quick-chat' } {
  return workspace?.kind === 'quick-chat'
}

export function getUserManagedProjectGroups(
  projectGroups: readonly ProjectGroup[]
): ProjectGroup[] {
  return projectGroups.filter((group) => !isQuickChatProjectGroup(group))
}

export function getQuickChatsRootPath(floatingWorkspacePath: string): string {
  return resolveRuntimePath(floatingWorkspacePath, QUICK_CHATS_DIRECTORY_NAME)
}

export function getQuickChatDirectoryPath(
  floatingWorkspacePath: string,
  quickChatId: string
): string {
  if (!SAFE_QUICK_CHAT_ID.test(quickChatId)) {
    throw new Error('Invalid quick chat ID')
  }
  return resolveRuntimePath(getQuickChatsRootPath(floatingWorkspacePath), quickChatId)
}

export function getActiveQuickChatWorkspace(state: {
  activeWorkspaceKey?: string | null
  activeWorktreeId?: string | null
  folderWorkspaces: readonly FolderWorkspace[]
}): FolderWorkspace | null {
  return getQuickChatWorkspaceForKey(
    state.activeWorkspaceKey ?? state.activeWorktreeId ?? '',
    state.folderWorkspaces
  )
}

export function getQuickChatWorkspaceForKey(
  workspaceKey: string,
  folderWorkspaces: readonly FolderWorkspace[]
): FolderWorkspace | null {
  const scope = parseWorkspaceKey(workspaceKey)
  if (scope?.type !== 'folder') {
    return null
  }
  const workspace = folderWorkspaces.find((entry) => entry.id === scope.folderWorkspaceId)
  return isQuickChatWorkspace(workspace) ? workspace : null
}
