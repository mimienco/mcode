import { afterEach, expect, it, vi } from 'vitest'
import { createTestStore } from '../store/slices/store-test-helpers'
import { createTauriLocalWorkspaceClient } from './tauri-local-workspace-client'
import { getDefaultSettings } from '../../../shared/constants'
import type { Repo } from '../../../shared/repo-types'

vi.mock('sonner', () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }))
afterEach(() => vi.unstubAllGlobals())

it('hydrates native catalogs through real store actions, discovers a project, and activates its checkout', async () => {
  const settings = getDefaultSettings('/home/test')
  const repo: Repo = { id: 'native-repo', path: '/projects/native', kind: 'git', displayName: 'Native', badgeColor: '#737373', addedAt: 1 }
  const savedRepos: Repo[] = []
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === 'workspace_catalog_get') {
      return { repos: savedRepos, projectGroups: [], folderWorkspaces: [] }
    }
    if (command === 'workspace_pick_folder') { return repo.path }
    if (command === 'workspace_open') {
      savedRepos.push(repo)
      return { kind: 'repo', repo, folderWorkspace: null, alreadyExisted: false }
    }
    if (command === 'workspace_list_detected') {
      const request = args?.args as { providerRequestId: string }
      return { status: 'complete', providerRequestId: request.providerRequestId, repoId: repo.id,
        authority: { kind: 'local', executionHostId: 'local' }, result: {
          repoId: repo.id, source: 'git', authoritative: true, worktrees: [{
            id: `${repo.id}::${repo.path}`, repoId: repo.id, path: repo.path, hostId: 'local',
            displayName: 'Native', head: 'abc', branch: 'refs/heads/main', isBare: false, isMainWorktree: true,
            comment: '', linkedIssue: null, linkedPR: null, linkedLinearIssue: null,
            isArchived: false, isUnread: false, isPinned: false, sortOrder: 0, lastActivityAt: 0
          }]
        } }
    }
    throw new Error(`Unexpected native command ${command}`)
  })
  const native = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
  vi.stubGlobal('window', { api: {
    ...native,
    gh: {},
    mcodeProfiles: { findProjectPresence: async () => [] },
    runtime: { call: async () => ({ id: 'unsupported', ok: false, error: { code: 'method_not_found', message: 'Not implemented' } }) }
  } })
  const store = createTestStore()
  store.setState({ settings })
  await store.getState().fetchReposForAllHosts({ remoteHosts: 'skip' })
  await store.getState().awaitLocalRepoCatalogSettlement()
  expect(store.getState().repos).toEqual([])
  await expect(store.getState().addRepo()).resolves.toMatchObject({ id: repo.id })
  await store.getState().fetchReposForAllHosts({ remoteHosts: 'skip' })
  await store.getState().fetchProjectGroupsForAllHosts({ remoteHosts: 'skip' })
  await store.getState().fetchFolderWorkspacesForAllHosts({ remoteHosts: 'skip' })
  expect(store.getState().projects).toEqual(expect.arrayContaining([expect.objectContaining({ id: 'repo:native-repo' })]))
  await expect(store.getState().fetchWorktrees(repo.id, { executionHostId: 'local' })).resolves.toBe(true)
  const checkout = store.getState().worktreesByRepo[repo.id]?.[0]
  expect(checkout).toMatchObject({ id: `${repo.id}::${repo.path}`, hostId: 'local', path: repo.path })
  store.getState().setActiveWorktree(checkout!.id)
  expect(store.getState().activeWorktreeId).toBe(checkout!.id)
  expect(invoke).toHaveBeenCalledWith('workspace_open', { args: { folderPath: repo.path, kind: 'git' } })
})

it('retains saved SSH groups and folders while local hydration preserves existing runtime owners', async () => {
  const sshGroup = { id: 'ssh-group', name: 'Remote folders', parentPath: '/remote', parentGroupId: null, connectionId: 'ssh-box', createdFrom: 'manual' as const, tabOrder: 0, isCollapsed: false, color: null, createdAt: 1, updatedAt: 1 }
  const runtimeGroup = { ...sshGroup, id: 'runtime-group', connectionId: null, executionHostId: 'runtime:env-1' }
  const remoteFolder = { id: 'ssh-folder', projectGroupId: sshGroup.id, name: 'Remote task', folderPath: '/remote/task', createdAt: 1, updatedAt: 1, sortOrder: 0 }
  const native = createTauriLocalWorkspaceClient(async () => ({ repos: [], projectGroups: [sshGroup], folderWorkspaces: [remoteFolder] }), {
    getSettings: async () => getDefaultSettings('/home/test')
  })
  vi.stubGlobal('window', { api: native })
  const store = createTestStore()
  store.setState({ projectGroups: [runtimeGroup] })
  await store.getState().fetchProjectGroupsForAllHosts({ remoteHosts: 'skip' })
  await store.getState().fetchFolderWorkspacesForAllHosts({ remoteHosts: 'skip' })
  expect(store.getState().projectGroups).toEqual(expect.arrayContaining([
    expect.objectContaining({ id: 'ssh-group', executionHostId: 'ssh:ssh-box' }),
    expect.objectContaining({ id: 'runtime-group', executionHostId: 'runtime:env-1' })
  ]))
  expect(store.getState().folderWorkspaces).toEqual(expect.arrayContaining([
    expect.objectContaining({ id: 'ssh-folder', projectGroupId: 'ssh-group' })
  ]))
})
