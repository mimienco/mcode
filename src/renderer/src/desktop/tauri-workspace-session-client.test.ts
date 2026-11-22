import assert from 'node:assert/strict'
import { describe, expect, it, vi } from 'vitest'
import {
  createTauriWorkspaceSessionClient,
  type WorkspaceOpenArgs,
  type WorkspaceRecents
} from './tauri-workspace-session-client'
import { DesktopCommandError } from './desktop-client'

function recordedInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler(args ?? {})
  }
  return { calls, invoke }
}

const repoRecord = {
  id: 'repo-1',
  path: 'D:/repo/alpha',
  displayName: 'Alpha',
  badgeColor: '#737373',
  addedAt: 1_000,
  kind: 'git'
}

const folderWorkspaceRecord = {
  id: 'fw-1',
  projectGroupId: 'group-1',
  name: 'Acme workspace',
  folderPath: 'D:/work/site',
  connectionId: null,
  linkedTask: null,
  comment: '',
  isArchived: false,
  isUnread: false,
  isPinned: false,
  sortOrder: 2_000,
  lastActivityAt: 0,
  createdAt: 2_000,
  updatedAt: 2_000
}

describe('tauri-workspace-session-client', () => {
  it('maps open onto workspace_open with camelCase args and returns the record', async () => {
    const { calls, invoke } = recordedInvoke({
      workspace_open: () => ({ kind: 'repo', repo: repoRecord, folderWorkspace: null, alreadyExisted: false })
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    const args: WorkspaceOpenArgs = {
      folderPath: 'D:/repo/alpha',
      kind: 'git',
      displayName: 'Alpha',
      hostId: 'local'
    }
    const record = await client.open(args)

    assert.deepEqual(record, {
      kind: 'repo',
      repo: repoRecord,
      folderWorkspace: null,
      alreadyExisted: false
    })
    expect(calls).toEqual([{ command: 'workspace_open', args: { args } }])
  })

  it('open returns a folder-workspace record for project-group opens', async () => {
    const { invoke } = recordedInvoke({
      workspace_open: () => ({
        kind: 'folderWorkspace',
        repo: null,
        folderWorkspace: folderWorkspaceRecord,
        alreadyExisted: false
      })
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    const record = await client.open({
      folderPath: 'D:/work/site',
      projectGroupId: 'group-1',
      kind: 'folder'
    })

    assert.equal(record.kind, 'folderWorkspace')
    assert.deepEqual(record.folderWorkspace, folderWorkspaceRecord)
    assert.equal(record.repo, null)
  })

  it('open propagates the typed error for an unauthorized path', async () => {
    const { invoke } = recordedInvoke({
      workspace_open: () => {
        throw {
          code: 'fs_path_denied',
          context: { operation: 'workspace_open' },
          message: 'path resolves outside the allowed directories',
          userMessage: 'MCode could not open this workspace. Try again.'
        }
      }
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    await expect(client.open({ folderPath: 'E:/outside' })).rejects.toMatchObject({
      code: 'fs_path_denied'
    })
  })

  it('open rejects a malformed payload with the typed invalid-response error', async () => {
    const { invoke } = recordedInvoke({
      workspace_open: () => ({ nonsense: true })
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    const error = await client.open({ folderPath: 'D:/repo/alpha' }).then(
      () => null,
      (thrown: unknown) => thrown
    )
    assert.ok(error instanceof DesktopCommandError)
    assert.equal(error.code, 'workspace_open_invalid_response')
  })

  it('maps switch onto workspace_switch with the pointer payload verbatim', async () => {
    const { calls, invoke } = recordedInvoke({
      workspace_switch: () => null
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    await client.switch({
      activeRepoId: 'repo-1',
      activeWorktreeId: 'worktree-1',
      activeWorkspaceKey: 'worktree:worktree-1',
      activeTabId: 'tab-1',
      visitedAt: 5_000
    })

    expect(calls).toEqual([
      {
        command: 'workspace_switch',
        args: {
          args: {
            activeRepoId: 'repo-1',
            activeWorktreeId: 'worktree-1',
            activeWorkspaceKey: 'worktree:worktree-1',
            activeTabId: 'tab-1',
            visitedAt: 5_000
          }
        }
      }
    ])
  })

  it('switch forwards a host partition id for non-local sessions', async () => {
    const { calls, invoke } = recordedInvoke({
      workspace_switch: () => null
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    await client.switch({
      activeWorktreeId: 'wt-ssh',
      activeWorkspaceExecutionHostId: 'ssh:box',
      hostId: 'ssh:box'
    })

    expect(calls[0]?.args).toEqual({
      args: {
        activeWorktreeId: 'wt-ssh',
        activeWorkspaceExecutionHostId: 'ssh:box',
        hostId: 'ssh:box'
      }
    })
  })

  it('maps listRecent onto workspace_list_recent with null for the local partition', async () => {
    const recents: WorkspaceRecents = {
      repos: [repoRecord],
      folderWorkspaces: [folderWorkspaceRecord],
      lastVisitedAtByWorktreeId: { 'folder:fw-1': 99 },
      activeWorkspaceKey: 'folder:fw-1',
      activeRepoId: 'repo-1'
    }
    const { calls, invoke } = recordedInvoke({
      workspace_list_recent: () => recents
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    const result = await client.listRecent()

    assert.deepEqual(result, recents)
    expect(calls).toEqual([{ command: 'workspace_list_recent', args: { hostId: null } }])
  })

  it('listRecent forwards a non-local host id', async () => {
    const { calls, invoke } = recordedInvoke({
      workspace_list_recent: () => ({
        repos: [],
        folderWorkspaces: [],
        lastVisitedAtByWorktreeId: {},
        activeWorkspaceKey: null,
        activeRepoId: null
      })
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    await client.listRecent('ssh:box')

    expect(calls).toEqual([{ command: 'workspace_list_recent', args: { hostId: 'ssh:box' } }])
  })

  it('listRecent rejects a malformed payload with the typed invalid-response error', async () => {
    const { invoke } = recordedInvoke({
      workspace_list_recent: () => ({ repos: 'not-an-array' })
    })
    const client = createTauriWorkspaceSessionClient(invoke)

    const error = await client.listRecent().then(
      () => null,
      (thrown: unknown) => thrown
    )
    assert.ok(error instanceof DesktopCommandError)
    assert.equal(error.code, 'workspace_list_recent_invalid_response')
  })

  it('normalizes generic invoke failures onto DesktopCommandError', async () => {
    const invoke = vi.fn(async () => {
      throw new Error('command not found')
    })
    const client = createTauriWorkspaceSessionClient(invoke as never)

    await expect(client.switch({})).rejects.toMatchObject({ code: 'desktop_command_failed' })
  })
})
