import { describe, expect, it } from 'vitest'
import type { FolderWorkspace } from './folder-workspace-types'
import type { ProjectGroup } from './project-group-types'
import {
  getQuickChatDirectoryPath,
  getQuickChatsRootPath,
  getActiveQuickChatWorkspace,
  getUserManagedProjectGroups,
  isQuickChatProjectGroup,
  isQuickChatWorkspace
} from './quick-chat-workspaces'
import { createProjectGroup, normalizeProjectGroups } from './project-groups'
import { normalizeFolderWorkspaces } from './folder-workspaces'

describe('quick chat workspace identity', () => {
  it('recognizes only explicitly marked system groups and workspaces', () => {
    expect(isQuickChatProjectGroup({ purpose: 'quick-chats' } as ProjectGroup)).toBe(true)
    expect(isQuickChatProjectGroup({ name: 'Chats' } as ProjectGroup)).toBe(false)
    expect(isQuickChatWorkspace({ kind: 'quick-chat' } as FolderWorkspace)).toBe(true)
    expect(isQuickChatWorkspace({ name: 'New chat' } as FolderWorkspace)).toBe(false)
  })

  it('keeps internal chat groups out of user-managed group lists', () => {
    const regular = { id: 'regular' } as ProjectGroup
    const quickChats = { id: 'chats', purpose: 'quick-chats' } as ProjectGroup
    expect(getUserManagedProjectGroups([regular, quickChats])).toEqual([regular])
  })
})

describe('quick chat workspace paths', () => {
  it('creates stable child paths for POSIX roots', () => {
    expect(getQuickChatsRootPath('/data/floating-workspace/')).toBe(
      '/data/floating-workspace/chats'
    )
    expect(getQuickChatDirectoryPath('/data/floating-workspace/', 'chat-123')).toBe(
      '/data/floating-workspace/chats/chat-123'
    )
  })

  it('preserves Windows separators', () => {
    expect(getQuickChatsRootPath('C:\\Users\\Ada\\MCode\\floating-workspace\\')).toBe(
      'C:/Users/Ada/MCode/floating-workspace/chats'
    )
  })

  it('rejects unsafe directory identifiers', () => {
    expect(() => getQuickChatDirectoryPath('/data/floating-workspace', '../escape')).toThrow(
      'Invalid quick chat ID'
    )
  })
})

describe('active quick chat resolution', () => {
  it('returns the active marked folder workspace', () => {
    const workspace = {
      id: 'chat-1',
      kind: 'quick-chat'
    } as FolderWorkspace
    expect(
      getActiveQuickChatWorkspace({
        activeWorkspaceKey: 'folder:chat-1',
        folderWorkspaces: [workspace]
      })
    ).toBe(workspace)
  })
})

describe('quick chat persistence fields', () => {
  it('round-trips the system group purpose and workspace kind', () => {
    const group = createProjectGroup({
      name: 'Chats',
      parentPath: '/app/floating-workspace/chats',
      parentGroupId: null,
      connectionId: null,
      createdFrom: 'manual',
      purpose: 'quick-chats',
      tabOrder: 0,
      now: 1
    })
    const normalizedGroups = normalizeProjectGroups([group])
    const normalizedWorkspaces = normalizeFolderWorkspaces(
      [
        {
          id: 'chat-1',
          projectGroupId: group.id,
          name: 'New chat',
          folderPath: '/app/floating-workspace/chats/chat-1',
          kind: 'quick-chat',
          linkedTask: null,
          comment: '',
          isArchived: false,
          isUnread: false,
          isPinned: false,
          sortOrder: 1,
          lastActivityAt: 0,
          createdAt: 1,
          updatedAt: 1
        }
      ],
      normalizedGroups
    )

    expect(normalizedGroups[0]?.purpose).toBe('quick-chats')
    expect(normalizedWorkspaces[0]?.kind).toBe('quick-chat')
  })
})
