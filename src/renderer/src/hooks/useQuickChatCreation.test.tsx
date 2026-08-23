// @vitest-environment happy-dom

import React, { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { FolderWorkspace } from '../../../shared/folder-workspace-types'
import type { ProjectGroup } from '../../../shared/project-group-types'
import { folderWorkspaceKey } from '../../../shared/workspace-scope'
import { useAppStore } from '@/store'
import { useQuickChatCreation } from './useQuickChatCreation'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const quickChatGroup: ProjectGroup = {
  id: 'quick-chats-group',
  name: 'Chats',
  parentPath: '/tmp/floating/chats',
  connectionId: null,
  parentGroupId: null,
  createdFrom: 'manual',
  purpose: 'quick-chats',
  tabOrder: 0,
  isCollapsed: false,
  color: null,
  createdAt: 1,
  updatedAt: 1
}

function Harness(): React.JSX.Element {
  const { createQuickChat, creating } = useQuickChatCreation()
  return (
    <button disabled={creating} onClick={createQuickChat}>
      New chat
    </button>
  )
}

let container: HTMLDivElement
let root: Root
let ensureDetectedAgents: ReturnType<typeof vi.fn>
let createFolderWorkspace: ReturnType<typeof vi.fn>

beforeEach(() => {
  ensureDetectedAgents = vi.fn(async () => ['claude'])
  createFolderWorkspace = vi.fn(
    async (input): Promise<FolderWorkspace> => ({
      id: 'quick-chat-1',
      projectGroupId: input.projectGroupId,
      name: input.name,
      folderPath: input.folderPath,
      kind: input.kind,
      linkedTask: input.linkedTask,
      comment: '',
      isArchived: false,
      isUnread: false,
      isPinned: false,
      sortOrder: 0,
      lastActivityAt: 1,
      createdAt: 1,
      updatedAt: 1
    })
  )

  useAppStore.setState({
    settings: { defaultTuiAgent: 'claude' } as never,
    projectGroups: [quickChatGroup],
    folderWorkspaces: [],
    tabsByWorktree: {},
    ensureDetectedAgents: ensureDetectedAgents as never,
    getFreshFolderWorkspacePathStatus: (() => ({ exists: true })) as never
  })

  Object.defineProperty(window, 'api', {
    configurable: true,
    value: {
      app: {
        getFloatingTerminalCwd: vi.fn(async () => '/tmp/floating')
      },
      fs: {
        pathExists: vi.fn(async () => true),
        createDir: vi.fn(async () => undefined),
        deletePath: vi.fn(async () => undefined)
      },
      folderWorkspaces: {
        create: createFolderWorkspace
      },
      projectGroups: {
        create: vi.fn()
      }
    }
  })

  container = document.createElement('div')
  document.body.append(container)
  root = createRoot(container)
})

afterEach(() => {
  act(() => root.unmount())
  container.remove()
})

describe('useQuickChatCreation', () => {
  it('opens an explicitly empty chat without selecting or launching an agent', async () => {
    act(() => root.render(<Harness />))

    act(() => container.querySelector('button')?.click())

    await act(async () => {
      await vi.waitFor(() => expect(createFolderWorkspace).toHaveBeenCalledOnce())
    })
    const createInput = createFolderWorkspace.mock.calls[0]?.[0]
    expect(createInput).not.toHaveProperty('createdWithAgent')
    expect(createInput).not.toHaveProperty('pendingFirstAgentMessageRename')
    expect(ensureDetectedAgents).not.toHaveBeenCalled()
    expect(useAppStore.getState().tabsByWorktree[folderWorkspaceKey('quick-chat-1')]).toEqual([])
  })
})
