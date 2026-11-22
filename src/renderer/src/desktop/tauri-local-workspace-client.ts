import type { RepositoryApi, ProjectGroupsApi, ProjectsApi } from '../../../preload/api/repository-api'
import type { FolderWorkspacesApi, WorktreeApi } from '../../../preload/api/worktree-api'
import type { Repo } from '../../../shared/repo-types'
import type { ProjectGroup } from '../../../shared/project-group-types'
import type { FolderWorkspace } from '../../../shared/folder-workspace-types'
import type { Worktree } from '../../../shared/worktree/types'
import type { WorktreeMeta } from '../../../shared/worktree/meta-types'
import type { GlobalSettings } from '../../../shared/global-settings-types'
import type { HostQualifiedDetectedWorktreeResult, ListDetectedWorktreesArgs, LegacyDetectedWorktreeRequest, ProviderRequestId } from '../../../shared/detected-worktree-provider-contract'
import { projectHostSetupProjectionFromRepos } from '../../../shared/project-host-setup-projection'
import { buildKnownMCodeWorkspaceLayouts, toDetectedWorktree } from '../../../shared/worktree/ownership'
import { getRepoExecutionHostId } from '../../../shared/execution-host'
import { createDesktopInvoke, DesktopCommandError, type DesktopInvoke } from './desktop-client'
import { createTauriWorkspaceSessionClient } from './tauri-workspace-session-client'

type Catalog = { repos: Repo[]; projectGroups: ProjectGroup[]; folderWorkspaces: FolderWorkspace[] }
type VisibilitySettings = Pick<GlobalSettings, 'workspaceDir' | 'nestWorkspaces' | 'workspaceDirHistory' | 'worktreeVisibilityDefaults'>
type Options = { getSettings: () => Promise<VisibilitySettings> }

function invalid(operation: string): DesktopCommandError {
  return new DesktopCommandError('workspace_invalid_response', { operation },
    `${operation} returned an invalid or mismatched workspace response.`, 'MCode could not load this workspace.')
}

function unavailable(operation: string): never {
  throw new DesktopCommandError('workspace_operation_unsupported', { operation },
    `${operation} is not available in this Tauri build.`, 'This workspace operation is not available yet.')
}

function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function validRepo(value: unknown): value is Repo {
  return object(value) && typeof value.id === 'string' && typeof value.path === 'string'
    && typeof value.displayName === 'string' && typeof value.badgeColor === 'string'
    && typeof value.addedAt === 'number'
}

function validWorktree(value: unknown, repoId: string): value is Worktree {
  return object(value) && typeof value.id === 'string' && value.repoId === repoId
    && typeof value.path === 'string' && typeof value.isMainWorktree === 'boolean'
    && (value.hostId === undefined || value.hostId === 'local')
}

export function createTauriLocalWorkspaceClient(invoke: DesktopInvoke, options: Options) {
  const call = createDesktopInvoke(invoke)
  const workspace = createTauriWorkspaceSessionClient(invoke)
  const repoListeners = new Set<() => void>()
  let nextRequestId = 0
  let catalogPending: Promise<Catalog> | null = null

  const catalog = (): Promise<Catalog> => {
    if (catalogPending) {
      return catalogPending
    }
    const pending = call('workspace_catalog_get').then((value): Catalog => {
      if (!object(value) || !Array.isArray(value.repos) || !value.repos.every(validRepo)
        || !Array.isArray(value.projectGroups) || !value.projectGroups.every((group) => object(group) && typeof group.id === 'string' && typeof group.name === 'string')
        || !Array.isArray(value.folderWorkspaces) || !value.folderWorkspaces.every((folder) => object(folder) && typeof folder.id === 'string' && typeof folder.projectGroupId === 'string')) {
        throw invalid('workspace_catalog_get')
      }
      if (value.repos.some((repo) => getRepoExecutionHostId(repo).startsWith('runtime:'))) {
        throw invalid('workspace_catalog_get')
      }
      return value as Catalog
    })
    catalogPending = pending
    void pending.finally(() => {
      if (catalogPending === pending) { catalogPending = null }
    }).catch(() => {})
    return pending
  }

  const pickFolder: RepositoryApi['pickFolder'] = async () => {
    const path = await call('workspace_pick_folder')
    if (path !== null && typeof path !== 'string') { throw invalid('workspace_pick_folder') }
    return path
  }

  const add: RepositoryApi['add'] = async ({ path, kind }) => {
    const result = await workspace.open({ folderPath: path, kind })
    if (result.kind !== 'repo' || !validRepo(result.repo) || getRepoExecutionHostId(result.repo) !== 'local') {
      throw invalid('workspace_open')
    }
    catalogPending = null
    for (const listener of repoListeners) { listener() }
    return { repo: result.repo }
  }

  const readDetected = async (request: ListDetectedWorktreesArgs): Promise<HostQualifiedDetectedWorktreeResult> => {
    if (request.executionHostId !== 'local') {
      return { status: 'rejected', providerRequestId: request.providerRequestId, executionHostId: request.executionHostId }
    }
    const [response, snapshot, settings] = await Promise.all([
      call('workspace_list_detected', { args: request }), catalog(), options.getSettings()
    ])
    if (!object(response) || response.providerRequestId !== request.providerRequestId) {
      throw invalid('workspace_list_detected')
    }
    if (response.status !== 'complete' && response.status !== 'non-authoritative') {
      if (response.executionHostId === 'local' && ['canceled', 'timed-out', 'stale', 'ambiguous-owner', 'authority-unknown', 'rejected'].includes(String(response.status))) {
        return response as HostQualifiedDetectedWorktreeResult
      }
      throw invalid('workspace_list_detected')
    }
    const result = response.result
    if (response.repoId !== request.repoId || !object(response.authority)
      || response.authority.kind !== 'local' || response.authority.executionHostId !== 'local'
      || !object(result) || result.repoId !== request.repoId || typeof result.authoritative !== 'boolean'
      || !['git', 'metadata-fallback', 'session-fallback'].includes(String(result.source))
      || !Array.isArray(result.worktrees) || !result.worktrees.every((row) => validWorktree(row, request.repoId))) {
      throw invalid('workspace_list_detected')
    }
    const repo = snapshot.repos.find((entry) => entry.id === request.repoId && getRepoExecutionHostId(entry) === 'local')
    if (!repo) { throw invalid('workspace_list_detected') }
    const knownMCodeLayouts = buildKnownMCodeWorkspaceLayouts(settings, repo)
    return {
      ...(response as HostQualifiedDetectedWorktreeResult & { status: 'complete' | 'non-authoritative' }),
      result: {
        ...(result as unknown as Extract<HostQualifiedDetectedWorktreeResult, { result: unknown }>['result']),
        worktrees: result.worktrees.map((row) => toDetectedWorktree({
          repo, worktree: row, meta: row as WorktreeMeta, settings, knownMCodeLayouts
        }))
      }
    }
  }

  const list = async ({ repoId }: { repoId: string }): Promise<Worktree[]> => {
    const result = await readDetected({ repoId, executionHostId: 'local', providerRequestId: `tauri-list-${++nextRequestId}` as ProviderRequestId })
    if (!('result' in result)) { throw invalid('workspace_list_detected') }
    return result.result.worktrees.filter((row) => row.visible)
  }

  const listDetected: WorktreeApi['listDetected'] = (async (request: ListDetectedWorktreesArgs | LegacyDetectedWorktreeRequest) => {
    if ('providerRequestId' in request) { return readDetected(request) }
    const response = await readDetected({ ...request, executionHostId: 'local', providerRequestId: `tauri-legacy-${++nextRequestId}` as ProviderRequestId })
    if (!('result' in response)) { throw invalid('workspace_list_detected') }
    return response.result
  }) as WorktreeApi['listDetected']

  const repos = {
    list: async () => (await catalog()).repos,
    add, pickFolder, pickDirectory: pickFolder,
    remove: async () => unavailable('repos.remove'),
    update: async () => unavailable('repos.update'),
    reorder: async () => unavailable('repos.reorder'),
    create: async () => unavailable('repos.create'),
    clone: async () => unavailable('repos.clone'),
    addRemote: async () => unavailable('repos.addRemote'),
    createRemote: async () => unavailable('repos.createRemote'),
    cloneRemote: async () => unavailable('repos.cloneRemote'),
    removeForHost: async () => unavailable('repos.removeForHost'),
    reorderForHost: async () => unavailable('repos.reorderForHost'),
    onChanged: (listener: () => void) => {
      repoListeners.add(listener)
      return () => { repoListeners.delete(listener) }
    }
  } satisfies Partial<RepositoryApi>
  const projects = {
    list: async () => [...projectHostSetupProjectionFromRepos((await catalog()).repos).projects],
    listHostSetups: async () => [...projectHostSetupProjectionFromRepos((await catalog()).repos).setups],
    update: async () => unavailable('projects.update'),
    createHostSetup: async () => unavailable('projects.createHostSetup'),
    setupExistingFolder: async () => unavailable('projects.setupExistingFolder'),
    updateHostSetup: async () => unavailable('projects.updateHostSetup'),
    deleteHostSetup: async () => unavailable('projects.deleteHostSetup')
  } satisfies Partial<ProjectsApi>
  const projectGroups = {
    list: async () => (await catalog()).projectGroups,
    create: async () => unavailable('projectGroups.create'),
    update: async () => unavailable('projectGroups.update'),
    delete: async () => unavailable('projectGroups.delete'),
    moveProject: async () => unavailable('projectGroups.moveProject'),
    importNested: async () => unavailable('projectGroups.importNested')
  } satisfies Partial<ProjectGroupsApi>
  const folderWorkspaces = {
    list: async () => (await catalog()).folderWorkspaces,
    create: async () => unavailable('folderWorkspaces.create'),
    update: async () => unavailable('folderWorkspaces.update'),
    delete: async () => unavailable('folderWorkspaces.delete'),
    getPathStatus: async (args) => {
      const result = await call('workspace_path_status', { args })
      if (!object(result) || typeof result.path !== 'string' || typeof result.exists !== 'boolean'
        || (result.reason !== undefined && !['missing', 'not-directory', 'unavailable', 'ambiguous-connection'].includes(String(result.reason)))) {
        throw invalid('workspace_path_status')
      }
      return result as Awaited<ReturnType<FolderWorkspacesApi['getPathStatus']>>
    }
  } satisfies Partial<FolderWorkspacesApi>
  return { repos, projects, projectGroups, folderWorkspaces, worktrees: {
    list, listDetected,
    create: async () => unavailable('worktrees.create'),
    remove: async () => unavailable('worktrees.remove'),
    adoptProvisionedRoot: async () => unavailable('worktrees.adoptProvisionedRoot')
  } }
}
