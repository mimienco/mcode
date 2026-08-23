import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as GitRunner from '../git/runner'
import type * as RepoModule from '../git/repo'

const { reposMocks, moduleMocks } = await vi.hoisted(async () => {
  const moduleMocks = await import('./repos-remote-test-harness')
  return { reposMocks: moduleMocks.createReposIpcMocks(), moduleMocks }
})

vi.mock('electron', () => moduleMocks.electronModuleMock(reposMocks))
vi.mock('../git/repo', async (importOriginal) =>
  moduleMocks.gitRepoModuleMock(await importOriginal<typeof RepoModule>())
)
vi.mock('../git/runner', async (importOriginal) =>
  moduleMocks.gitRunnerModuleMock(reposMocks, await importOriginal<typeof GitRunner>())
)
vi.mock('../git/worktree', () => moduleMocks.gitWorktreeModuleMock(reposMocks))
vi.mock('./registered-worktree-roots-cache', () =>
  moduleMocks.registeredWorktreeRootsCacheModuleMock(reposMocks)
)
vi.mock('../worktree-root-preparation', () =>
  moduleMocks.worktreeRootPreparationModuleMock(reposMocks)
)
vi.mock('../providers/ssh-git-dispatch', () => moduleMocks.sshGitDispatchModuleMock(reposMocks))
vi.mock('../providers/ssh-filesystem-dispatch', () =>
  moduleMocks.sshFilesystemDispatchModuleMock(reposMocks)
)
vi.mock('./ssh', () => moduleMocks.sshModuleMock(reposMocks))

import { registerRepoHandlers } from './repos'
import { createRepoHandlerHarness, resetLocalRepoMocks } from './repos-remote-test-harness'

const { handleMock, mockStore } = reposMocks

describe('quick-chat repository IPC boundaries', () => {
  const { handlers, mockWindow, captureHandlers } = createRepoHandlerHarness()
  const quickChatGroup = {
    id: 'quick-chats',
    name: 'Chats',
    parentPath: 'C:\\Users\\Ava\\AppData\\Local\\MCode\\chats',
    connectionId: null,
    parentGroupId: null,
    createdFrom: 'manual' as const,
    purpose: 'quick-chats' as const
  }

  beforeEach(() => {
    captureHandlers(handleMock)
    resetLocalRepoMocks(reposMocks)
    mockWindow.webContents.send.mockReset()
    registerRepoHandlers(mockWindow as never, mockStore as never)
  })

  it('persists a valid local quick-chat group marker', () => {
    mockStore.createProjectGroup.mockReturnValue(quickChatGroup)

    expect(
      handlers.get('projectGroups:create')!(null, {
        name: 'Chats',
        parentPath: quickChatGroup.parentPath,
        connectionId: null,
        parentGroupId: null,
        createdFrom: 'manual',
        purpose: 'quick-chats'
      })
    ).toBe(quickChatGroup)

    expect(mockStore.createProjectGroup).toHaveBeenCalledWith(
      expect.objectContaining({ purpose: 'quick-chats' })
    )
  })

  it('rejects remote and WSL-backed quick-chat groups', () => {
    expect(() =>
      handlers.get('projectGroups:create')!(null, {
        name: 'Chats',
        parentPath: '/srv/chats',
        connectionId: 'ssh:builder',
        purpose: 'quick-chats'
      })
    ).toThrow('invalid_quick_chat_project_group_scope')
    expect(() =>
      handlers.get('projectGroups:create')!(null, {
        name: 'Chats',
        parentPath: '\\\\wsl.localhost\\Ubuntu\\home\\ava\\chats',
        purpose: 'quick-chats'
      })
    ).toThrow('invalid_quick_chat_project_group_scope')
  })

  it('protects the system group from editing, deletion, and project assignment', () => {
    mockStore.getProjectGroups.mockReturnValue([quickChatGroup])

    expect(
      handlers.get('projectGroups:update')!(null, {
        groupId: quickChatGroup.id,
        updates: { name: 'Renamed' }
      })
    ).toBeNull()
    expect(handlers.get('projectGroups:delete')!(null, { groupId: quickChatGroup.id })).toBe(false)
    expect(() =>
      handlers.get('projectGroups:moveProject')!(null, {
        projectId: 'project-1',
        groupId: quickChatGroup.id,
        order: 0
      })
    ).toThrow('invalid_quick_chat_project_group_assignment')

    expect(mockStore.updateProjectGroup).not.toHaveBeenCalled()
    expect(mockStore.deleteProjectGroup).not.toHaveBeenCalled()
    expect(mockStore.moveProjectToGroup).not.toHaveBeenCalled()
  })

  it('rejects a regular folder workspace inside the quick-chat group', async () => {
    mockStore.getProjectGroups.mockReturnValue([quickChatGroup])

    await expect(
      handlers.get('folderWorkspaces:create')!(null, {
        projectGroupId: quickChatGroup.id,
        name: 'Not a chat',
        folderPath: `${quickChatGroup.parentPath}\\ordinary-workspace`,
        linkedTask: null
      })
    ).rejects.toThrow('invalid_quick_chat_workspace_scope')
  })
})
