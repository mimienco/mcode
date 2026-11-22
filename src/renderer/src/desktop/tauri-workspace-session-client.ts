import { createDesktopInvoke, DesktopCommandError } from './desktop-client'
import type { ExecutionHostId } from '../../../shared/execution-host'
import type { FolderWorkspace } from '../../../shared/folder-workspace-types'

/**
 * Workspace-session parity surface (parity row `workspace.session`): typed
 * seams over the Rust `workspace_open` / `workspace_switch` /
 * `workspace_list_recent` commands (`src-tauri/src/workspace_commands.rs`,
 * serde camelCase). DTOs mirror the Electron records field-for-field —
 * `Repo` (`src/shared/repo-types.ts`) and `FolderWorkspace`
 * (`src/shared/folder-workspace-types.ts`) — so callers can mix records from
 * this client with `window.api.repos` / `window.api.folderWorkspaces`
 * payloads unchanged.
 */

/** `RepoKind` subset the open command accepts (`repos.add`'s `{ kind }`). */
export type WorkspaceOpenKind = 'git' | 'folder'

/** Args of the Rust `workspace_open` command (camelCase boundary). */
export type WorkspaceOpenArgs = {
  /** Absolute local folder path to open. */
  folderPath: string
  /** Defaults to `'git'` on the Rust side, matching `repos:add`. */
  kind?: WorkspaceOpenKind
  /** Folder-workspace creation: attach the folder to this project group. */
  projectGroupId?: string
  /** Display-name override (Electron `repos:create` / `createFolderWorkspace`). */
  displayName?: string
  /** SSH/runtime target the folder lives on; omit for local opens. */
  connectionId?: string
  /** Session/host partition for the open; omit for `'local'`. */
  hostId?: ExecutionHostId
}

/** Record persisted for a repo open — the `Repo` add-time shape. */
export type WorkspaceOpenRepoRecord = {
  id: string
  path: string
  displayName: string
  badgeColor: string
  addedAt: number
  kind: WorkspaceOpenKind
  connectionId?: string
}

/** Record persisted for a folder-workspace open — the add-time shape. */
export type WorkspaceOpenFolderWorkspaceRecord = FolderWorkspace

export type WorkspaceOpenRecord = {
  kind: 'repo' | 'folderWorkspace'
  repo: WorkspaceOpenRepoRecord | null
  folderWorkspace: WorkspaceOpenFolderWorkspaceRecord | null
  /** `true` when an existing record already covered the path. */
  alreadyExisted: boolean
}

/** Args of the Rust `workspace_switch` command. */
export type WorkspaceSwitchArgs = {
  activeRepoId?: string | null
  /** Legacy worktree id or canonical `worktree:`/`folder:` WorkspaceKey. */
  activeWorktreeId?: string | null
  activeWorkspaceKey?: string | null
  activeWorkspaceExecutionHostId?: ExecutionHostId | null
  activeTabId?: string | null
  /** Focus-recency stamp written into `lastVisitedAtByWorktreeId`. */
  visitedAt?: number
  /** Session partition to write; omit for `'local'`. */
  hostId?: ExecutionHostId
}

/** `workspace_list_recent` result — Electron's order semantics. */
export type WorkspaceRecents = {
  /** Repo records in document order (`store.getRepos()` array order). */
  repos: Record<string, unknown>[]
  /** Folder workspaces sorted `sortOrder` desc, then `name` asc. */
  folderWorkspaces: FolderWorkspace[]
  /** Focus recency from the session partition; absent = never focused. */
  lastVisitedAtByWorktreeId: Record<string, number>
  activeWorkspaceKey: string | null
  activeRepoId: string | null
}

export type WorkspaceSessionInvoke = (
  command: string,
  args?: Record<string, unknown>
) => Promise<unknown>

/**
 * Typed client over the three workspace commands. Raw command strings live
 * only here; every failure is normalized onto `DesktopCommandError` via
 * `createDesktopInvoke` (fs-client/shell-client pattern).
 */
export function createTauriWorkspaceSessionClient(
  invoke: WorkspaceSessionInvoke
): {
  open: (args: WorkspaceOpenArgs) => Promise<WorkspaceOpenRecord>
  switch: (args: WorkspaceSwitchArgs) => Promise<void>
  listRecent: (hostId?: ExecutionHostId) => Promise<WorkspaceRecents>
} {
  const call = createDesktopInvoke(invoke)
  return {
    open: async (args) => {
      const record = (await call('workspace_open', { args })) as WorkspaceOpenRecord
      if (!record || (record.kind !== 'repo' && record.kind !== 'folderWorkspace')) {
        throw new DesktopCommandError(
          'workspace_open_invalid_response',
          { operation: 'workspace_open' },
          'workspace_open returned an unexpected payload',
          'MCode could not open this workspace. Try again.',
          record
        )
      }
      return record
    },
    switch: async (args) => {
      await call('workspace_switch', { args })
    },
    listRecent: async (hostId?: ExecutionHostId) => {
      const recents = (await call('workspace_list_recent', {
        hostId: hostId ?? null
      })) as WorkspaceRecents
      if (!recents || typeof recents !== 'object' || !Array.isArray(recents.repos)) {
        throw new DesktopCommandError(
          'workspace_list_recent_invalid_response',
          { operation: 'workspace_list_recent' },
          'workspace_list_recent returned an unexpected payload',
          'MCode could not list recent workspaces. Try again.',
          recents
        )
      }
      return recents
    }
  }
}
