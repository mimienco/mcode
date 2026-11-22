import { describe, expect, it, vi } from 'vitest'
import { createTauriLocalWorkspaceClient } from './tauri-local-workspace-client'
import type { Repo } from '../../../shared/repo-types'
import type { ProviderRequestId } from '../../../shared/detected-worktree-provider-contract'

const repo: Repo = { id: 'repo-1', path: '/repos/project', displayName: 'Project', badgeColor: '#737373', addedAt: 1, kind: 'git' }
const catalog = {
  repos: [repo],
  projectGroups: [{ id: 'group-1', name: 'Folders', parentPath: null, parentGroupId: null, createdFrom: 'manual', tabOrder: 0, isCollapsed: false, color: null, createdAt: 1, updatedAt: 1 }],
  folderWorkspaces: [{ id: 'folder-1', projectGroupId: 'group-1', name: 'Notes', folderPath: '/notes' }]
}
const request = { providerRequestId: 'request-1' as ProviderRequestId, repoId: repo.id, executionHostId: 'local' as const }
const settings = { workspaceDir: '/workspaces', nestWorkspaces: true }

function worktree(path: string, main = false) {
  return { id: `${repo.id}::${path}`, repoId: repo.id, path, displayName: 'Project', head: 'abc', branch: 'refs/heads/main', isMainWorktree: main, isBare: false, comment: '', linkedIssue: null, linkedPR: null, linkedLinearIssue: null, isArchived: false, isUnread: false, isPinned: false, sortOrder: 0, lastActivityAt: 0 }
}

function detected(rows = [worktree(repo.path, true)]) {
  return { status: 'complete', providerRequestId: request.providerRequestId, repoId: repo.id,
    authority: { kind: 'local', executionHostId: 'local' },
    result: { repoId: repo.id, authoritative: true, source: 'git', worktrees: rows } }
}

describe('native local workspace catalog adapter', () => {
  it('loads actual catalogs and derives stable project/setup identities without conflating folder groups', async () => {
    const invoke = vi.fn(async () => catalog)
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    const [repos, projects, setups, groups, folders] = await Promise.all([
      client.repos.list(), client.projects.list(), client.projects.listHostSetups(), client.projectGroups.list(), client.folderWorkspaces.list()
    ])
    expect(invoke).toHaveBeenCalledOnce()
    expect(repos).toEqual([repo])
    expect(projects[0]).toMatchObject({ id: 'repo:repo-1', sourceRepoIds: ['repo-1'] })
    expect(setups[0]).toMatchObject({ repoId: repo.id, hostId: 'local', path: repo.path, setupState: 'ready' })
    expect(groups).toEqual(catalog.projectGroups)
    expect(folders).toEqual(catalog.folderWorkspaces)
  })

  it('routes the normal picker/add contract and notifies catalog subscribers after persistence succeeds', async () => {
    const invoke = vi.fn(async (command: string) => command === 'workspace_pick_folder'
      ? repo.path : { kind: 'repo', repo, folderWorkspace: null, alreadyExisted: false })
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    const changed = vi.fn()
    const off = client.repos.onChanged(changed)
    const path = await client.repos.pickFolder()
    await expect(client.repos.add({ path: path!, kind: 'git' })).resolves.toEqual({ repo })
    expect(invoke).toHaveBeenLastCalledWith('workspace_open', { args: { folderPath: repo.path, kind: 'git' } })
    expect(changed).toHaveBeenCalledOnce()
    off()
    await client.repos.add({ path: repo.path })
    expect(changed).toHaveBeenCalledOnce()
  })

  it('keeps picker cancellation distinct from malformed responses and does not notify failed writes', async () => {
    const invoke = vi.fn(async () => null)
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    expect(await client.repos.pickFolder()).toBeNull()
    const changed = vi.fn()
    client.repos.onChanged(changed)
    invoke.mockRejectedValueOnce(new Error('Not a valid git repository'))
    await expect(client.repos.add({ path: '/not-git', kind: 'git' })).rejects.toThrow('Not a valid git repository')
    expect(changed).not.toHaveBeenCalled()
  })

  it('uses shared visibility rules on real scan rows and preserves literal worktree identity', async () => {
    const rows = [worktree(repo.path, true), worktree(`${repo.path}/.claude/worktrees/scratch`)]
    const invoke = vi.fn(async (command: string) => command === 'workspace_catalog_get' ? catalog : detected(rows))
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    const response = await client.worktrees.listDetected(request)
    expect(response).toMatchObject({ status: 'complete', result: { authoritative: true, source: 'git', worktrees: [
      { id: `${repo.id}::${repo.path}`, selectedCheckout: true, visible: true },
      { ownership: 'agent-scratch', visible: false }
    ] } })
    expect(invoke).toHaveBeenCalledWith('workspace_list_detected', { args: request })
  })

  it('rejects remote provider requests before invoking local filesystem commands', async () => {
    const invoke = vi.fn()
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    const remote = { ...request, executionHostId: 'ssh:remote' as const, expectedAuthority: {} as never }
    await expect(client.worktrees.listDetected(remote)).resolves.toEqual({ status: 'rejected', providerRequestId: request.providerRequestId, executionHostId: 'ssh:remote' })
    await expect(client.repos.addRemote()).rejects.toMatchObject({ code: 'workspace_operation_unsupported' })
    expect(invoke).not.toHaveBeenCalled()
  })

  it('rejects crossed request/host identities instead of admitting an authoritative empty scan', async () => {
    const response = detected()
    response.providerRequestId = 'other' as ProviderRequestId
    const invoke = vi.fn(async (command: string) => command === 'workspace_catalog_get' ? catalog : response)
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    await expect(client.worktrees.listDetected(request)).rejects.toMatchObject({ code: 'workspace_invalid_response' })
    invoke.mockImplementation(async (command) => command === 'workspace_catalog_get'
      ? { ...catalog, repos: [{ ...repo, executionHostId: 'runtime:remote' }] } : detected())
    await expect(client.repos.list()).rejects.toMatchObject({ code: 'workspace_invalid_response' })
  })

  it('retains direct SSH metadata without treating it as permission to scan that host locally', async () => {
    const remoteRepo = { ...repo, id: 'remote-repo', connectionId: 'remote', path: '/remote/project' }
    const invoke = vi.fn(async (_command: string) => ({ ...catalog, repos: [repo, remoteRepo] }))
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    expect(await client.repos.list()).toEqual([repo, remoteRepo])
    expect(await client.projects.listHostSetups()).toEqual(expect.arrayContaining([expect.objectContaining({ repoId: 'remote-repo', hostId: 'ssh:remote' })]))
    expect(invoke.mock.calls.every(([command]) => command === 'workspace_catalog_get')).toBe(true)
  })

  it('keeps unavailable path evidence distinct from confirmed missing folders', async () => {
    const invoke = vi.fn(async () => ({ path: '/remote/project', exists: false, reason: 'unavailable' }))
    const client = createTauriLocalWorkspaceClient(invoke, { getSettings: async () => settings })
    await expect(client.folderWorkspaces.getPathStatus({ scope: 'path', path: '/remote/project', connectionId: 'remote' })).resolves.toEqual({ path: '/remote/project', exists: false, reason: 'unavailable' })
    expect(invoke).toHaveBeenCalledWith('workspace_path_status', { args: { scope: 'path', path: '/remote/project', connectionId: 'remote' } })
  })
})
