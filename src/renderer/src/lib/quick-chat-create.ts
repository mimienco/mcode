import type { ProjectGroup } from '../../../shared/project-group-types'
import {
  getQuickChatDirectoryPath,
  getQuickChatsRootPath,
  isQuickChatProjectGroup,
  QUICK_CHATS_GROUP_NAME
} from '../../../shared/quick-chat-workspaces'
import { normalizeRuntimePathForComparison } from '../../../shared/cross-platform-path'

type QuickChatGroupCreateInput = {
  name: string
  parentPath: string
  connectionId: null
  parentGroupId: null
  createdFrom: 'manual'
  purpose: 'quick-chats'
}

export type PrepareQuickChatWorkspaceInput = {
  quickChatId: string
  projectGroups: readonly ProjectGroup[]
  getFloatingWorkspacePath: () => Promise<string>
  ensureDir: (dirPath: string) => Promise<void>
  createGroup: (input: QuickChatGroupCreateInput) => Promise<ProjectGroup>
}

export async function prepareQuickChatWorkspace(
  input: PrepareQuickChatWorkspaceInput
): Promise<{ projectGroup: ProjectGroup; folderPath: string }> {
  const floatingWorkspacePath = await input.getFloatingWorkspacePath()
  const quickChatsRootPath = getQuickChatsRootPath(floatingWorkspacePath)
  const folderPath = getQuickChatDirectoryPath(floatingWorkspacePath, input.quickChatId)
  await input.ensureDir(quickChatsRootPath)
  await input.ensureDir(folderPath)

  const quickChatsRootIdentity = normalizeRuntimePathForComparison(quickChatsRootPath)
  const existingGroup = input.projectGroups.find(
    (group) =>
      isQuickChatProjectGroup(group) &&
      group.parentPath != null &&
      normalizeRuntimePathForComparison(group.parentPath) === quickChatsRootIdentity
  )
  const projectGroup =
    existingGroup ??
    (await input.createGroup({
      name: QUICK_CHATS_GROUP_NAME,
      parentPath: quickChatsRootPath,
      connectionId: null,
      parentGroupId: null,
      createdFrom: 'manual',
      purpose: 'quick-chats'
    }))

  return { projectGroup, folderPath }
}
