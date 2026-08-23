import { describe, expect, it, vi } from 'vitest'
import type { ProjectGroup } from '../../../shared/project-group-types'
import { prepareQuickChatWorkspace } from './quick-chat-create'

function group(overrides: Partial<ProjectGroup> = {}): ProjectGroup {
  return {
    id: 'quick-chats-group',
    name: 'Chats',
    parentPath: '/app/floating-workspace/chats',
    connectionId: null,
    parentGroupId: null,
    createdFrom: 'manual',
    purpose: 'quick-chats',
    tabOrder: 0,
    isCollapsed: false,
    color: null,
    createdAt: 1,
    updatedAt: 1,
    ...overrides
  }
}

describe('prepareQuickChatWorkspace', () => {
  it('creates a persistent per-chat directory and the system group', async () => {
    const createdGroup = group()
    const ensureDir = vi.fn(async () => undefined)
    const createGroup = vi.fn(async () => createdGroup)

    await expect(
      prepareQuickChatWorkspace({
        quickChatId: 'chat-123',
        projectGroups: [],
        getFloatingWorkspacePath: async () => '/app/floating-workspace',
        ensureDir,
        createGroup
      })
    ).resolves.toEqual({
      projectGroup: createdGroup,
      folderPath: '/app/floating-workspace/chats/chat-123'
    })
    expect(ensureDir).toHaveBeenNthCalledWith(1, '/app/floating-workspace/chats')
    expect(ensureDir).toHaveBeenNthCalledWith(2, '/app/floating-workspace/chats/chat-123')
    expect(createGroup).toHaveBeenCalledWith({
      name: 'Chats',
      parentPath: '/app/floating-workspace/chats',
      connectionId: null,
      parentGroupId: null,
      createdFrom: 'manual',
      purpose: 'quick-chats'
    })
  })

  it('reuses the existing quick-chat group', async () => {
    const existingGroup = group()
    const createGroup = vi.fn()

    const result = await prepareQuickChatWorkspace({
      quickChatId: 'chat-456',
      projectGroups: [existingGroup],
      getFloatingWorkspacePath: async () => '/app/floating-workspace',
      ensureDir: async () => undefined,
      createGroup
    })

    expect(result.projectGroup).toBe(existingGroup)
    expect(createGroup).not.toHaveBeenCalled()
  })

  it('creates a new system group after the floating workspace root changes', async () => {
    const existingGroup = group({ parentPath: '/old/floating-workspace/chats' })
    const replacementGroup = group({
      id: 'new-quick-chats-group',
      parentPath: '/new/floating-workspace/chats'
    })
    const createGroup = vi.fn(async () => replacementGroup)

    const result = await prepareQuickChatWorkspace({
      quickChatId: 'chat-789',
      projectGroups: [existingGroup],
      getFloatingWorkspacePath: async () => '/new/floating-workspace',
      ensureDir: async () => undefined,
      createGroup
    })

    expect(result.projectGroup).toBe(replacementGroup)
    expect(createGroup).toHaveBeenCalledOnce()
  })
})
