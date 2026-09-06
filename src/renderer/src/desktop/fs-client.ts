import {
  createTauriFsExtraMethods,
  subscribeFsChanged,
  type FsExtraMethods
} from './fs-extra-client'

import { createDesktopInvoke } from './desktop-client'

// ─── Wire DTOs (mirror the Rust serde shapes in src-tauri) ──────────────────

export type FsDirEntry = {
  name: string
  isDirectory: boolean
  isSymlink: boolean
}

export type FsReadFileResult = {
  content: string
  isBinary: boolean
  mimeType?: string
}

export type FsStatResult = {
  size: number
  isDirectory: boolean
  mtime: number
}

export type FsSearchMatch = {
  path: string
  lineNumber: number
  lineText: string
}

export type FsSearchResult = {
  matches: FsSearchMatch[]
  truncated: boolean
  rgUnavailable?: boolean
}

export type FsReplaceOutcome =
  | { status: 'replaced'; replacements: number }
  | { status: 'skipped'; reason: string }
  | { status: 'failed'; error: string }

export type FsReplaceSummary = {
  outcomes: Record<string, FsReplaceOutcome>
  cancelled: boolean
}

export type FsWatchEvent = {
  kind: 'create' | 'update' | 'delete' | 'overflow'
  absolutePath: string
  oldAbsolutePath?: string
  isDirectory?: boolean
}

export type FsChangedPayload = {
  worktreePath: string
  events: FsWatchEvent[]
}

export type FsWatchSubscription = () => void

// ─── Typed API surface (one method per fs_* command) ────────────────────────

export type FsApiCore = {
  readDir: (args: { dirPath: string }) => Promise<FsDirEntry[]>
  readFile: (args: { filePath: string }) => Promise<FsReadFileResult>
  writeFile: (args: { filePath: string; content: string }) => Promise<void>
  pathExists: (args: { filePath: string }) => Promise<boolean>
  stat: (args: { filePath: string }) => Promise<FsStatResult>
  createDir: (args: { dirPath: string }) => Promise<void>
  createFile: (args: { filePath: string }) => Promise<void>
  copyPath: (args: { sourcePath: string; destinationPath: string }) => Promise<void>
  renamePath: (args: { oldPath: string; newPath: string }) => Promise<void>
  deletePath: (args: { targetPath: string }) => Promise<void>
  writeFileAtomic: (args: { filePath: string; content: string }) => Promise<void>
  search: (args: {
    rootPath: string
    query: string
    maxResults?: number
    requestToken?: string
  }) => Promise<FsSearchResult>
  searchReplace: (args: {
    rootPath: string
    query: string
    replacement: string
    skipPaths?: string[]
    requestId?: string
  }) => Promise<FsReplaceSummary>
  cancelListFiles: (args: { requestToken: string }) => Promise<void>
  cancelSearchReplace: (args: { requestId: string }) => Promise<void>
  watchWorktree: (args: { worktreePath: string }) => Promise<void>
  unwatchWorktree: (args: { worktreePath: string }) => Promise<void>
  onFsChanged: (listener: (payload: FsChangedPayload) => void) => () => void
}

/**
 * Full 34-method `window.api.fs` surface: the 21-command core plus the
 * Task-C remainder methods from `fs-extra-client`.
 */
export type FsApi = FsApiCore & FsExtraMethods

type FsInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

type FsEventSource = (
  event: string,
  listener: (payload: { payload: unknown }) => void
) => Promise<() => void>

/**
 * The native fs commands serialize `Option` error-context fields as JSON
 * `null`, but the shared `DesktopCommandError` guard treats only `undefined`
 * as absent. Nulls are stripped so the payload keeps its native code.
 */
export const FS_CHANGED_EVENT = 'fs://changed'

/**
 * Renderer-facing log-tail event name (spec §4 `fs:localLogTailChanged`).
 */
export const FS_LOG_TAIL_CHANGED_EVENT = 'fs://log-tail-changed'

function stripNullContextFields(error: unknown): unknown {
  if (
    !error ||
    typeof error !== 'object' ||
    !('context' in error) ||
    !error.context ||
    typeof error.context !== 'object' ||
    Array.isArray(error.context)
  ) {
    return error
  }
  const { context, ...rest } = error
  const cleaned = Object.fromEntries(Object.entries(context).filter(([, value]) => value !== null))
  return { ...rest, context: cleaned }
}

/**
 * Builds the typed fs API over any invoke transport. Raw command strings live
 * only here; callers receive one typed method per `fs_*` command and every
 * failure is normalized onto `DesktopCommandError`.
 */
/**
 * Bridges the preload's synchronous-unsubscribe onFsChanged contract onto
 * the async Tauri listener arming: payloads flow to the listener only after
 * arming; the returned unsubscribe works before and after.
 */
function syncUnsubscribeFsChanged(arm: () => Promise<FsWatchSubscription>): () => void {
  let realUnsubscribe: (() => void) | undefined
  let disposed = false
  void arm().then((unlisten) => {
    if (disposed) {
      unlisten()
    } else {
      realUnsubscribe = unlisten
    }
  })
  return () => {
    disposed = true
    realUnsubscribe?.()
  }
}

export function createTauriFsClient(
  invoke: FsInvoke,
  // The injected event source: Task-C event subscriptions and the guarded
  // fs://changed subscriber both route through it, so tests keep the seam.
  listen: FsEventSource
): FsApi {
  const desktopInvoke = createDesktopInvoke((command, args) =>
    invoke(command, args).catch((error: unknown) => {
      throw stripNullContextFields(error)
    })
  )
  const call = (command: string, args?: Record<string, unknown>): Promise<unknown> =>
    desktopInvoke(command, args)

  return {
    readDir: ({ dirPath }) => call('fs_read_dir', { dirPath }) as Promise<FsDirEntry[]>,
    readFile: ({ filePath }) => call('fs_read_file', { filePath }) as Promise<FsReadFileResult>,
    writeFile: ({ filePath, content }) =>
      call('fs_write_file', { filePath, content }).then(() => undefined),
    pathExists: ({ filePath }) => call('fs_path_exists', { filePath }) as Promise<boolean>,
    stat: ({ filePath }) => call('fs_stat', { filePath }) as Promise<FsStatResult>,
    createDir: ({ dirPath }) => call('fs_create_dir', { dirPath }).then(() => undefined),
    createFile: ({ filePath }) => call('fs_create_file', { filePath }).then(() => undefined),
    copyPath: ({ sourcePath, destinationPath }) =>
      call('fs_copy_path', { sourcePath, destinationPath }).then(() => undefined),
    renamePath: ({ oldPath, newPath }) =>
      call('fs_rename_path', { oldPath, newPath }).then(() => undefined),
    deletePath: ({ targetPath }) => call('fs_delete_path', { targetPath }).then(() => undefined),
    writeFileAtomic: ({ filePath, content }) =>
      call('fs_write_file_atomic', { filePath, content }).then(() => undefined),
    search: ({ rootPath, query, maxResults, requestToken }) =>
      call('fs_search', { rootPath, query, maxResults, requestToken }) as Promise<FsSearchResult>,
    searchReplace: ({ rootPath, query, replacement, skipPaths, requestId }) =>
      call('fs_search_replace', {
        rootPath,
        query,
        replacement,
        skipPaths,
        requestId
      }) as Promise<FsReplaceSummary>,
    cancelListFiles: ({ requestToken }) =>
      call('fs_cancel_list_files', { requestToken }).then(() => undefined),
    cancelSearchReplace: ({ requestId }) =>
      call('fs_cancel_search_replace', { requestId }).then(() => undefined),
    watchWorktree: ({ worktreePath }) =>
      call('fs_watch_worktree', { worktreePath }).then(() => undefined),
    unwatchWorktree: ({ worktreePath }) =>
      call('fs_unwatch_worktree', { worktreePath }).then(() => undefined),
    ...createTauriFsExtraMethods(call, listen),
    onFsChanged: (listener) => syncUnsubscribeFsChanged(() => subscribeFsChanged(listener, listen))
  }
}

/**
 * Runtime-selection wrapper: resolves to the Tauri-native fs client lazily on
 * first use under Tauri; under Electron callers supply their own bridge.
 */
export function createRuntimeFsClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri
}: {
  isTauri: () => boolean
  electron: FsApi
  loadTauri: () => Promise<FsApi>
}): FsApi {
  let tauriClient: Promise<FsApi> | undefined
  const resolveClient = (): Promise<FsApi> => {
    if (!isTauriRuntime()) {
      return Promise.resolve(electron)
    }
    tauriClient ??= loadTauri()
    return tauriClient
  }
  return {
    readDir: async (args) => (await resolveClient()).readDir(args),
    readFile: async (args) => (await resolveClient()).readFile(args),
    writeFile: async (args) => {
      await (await resolveClient()).writeFile(args)
    },
    pathExists: async (args) => (await resolveClient()).pathExists(args),
    stat: async (args) => (await resolveClient()).stat(args),
    createDir: async (args) => {
      await (await resolveClient()).createDir(args)
    },
    createFile: async (args) => {
      await (await resolveClient()).createFile(args)
    },
    copyPath: async (args) => {
      await (await resolveClient()).copyPath(args)
    },
    renamePath: async (args) => {
      await (await resolveClient()).renamePath(args)
    },
    deletePath: async (args) => {
      await (await resolveClient()).deletePath(args)
    },
    writeFileAtomic: async (args) => {
      await (await resolveClient()).writeFileAtomic(args)
    },
    search: async (args) => (await resolveClient()).search(args),
    searchReplace: async (args) => (await resolveClient()).searchReplace(args),
    cancelListFiles: async (args) => {
      await (await resolveClient()).cancelListFiles(args)
    },
    cancelSearchReplace: async (args) => {
      await (await resolveClient()).cancelSearchReplace(args)
    },
    watchWorktree: async (args) => {
      await (await resolveClient()).watchWorktree(args)
    },
    unwatchWorktree: async (args) => {
      await (await resolveClient()).unwatchWorktree(args)
    },
    authorizeExternalPath: async (args) => {
      await (await resolveClient()).authorizeExternalPath(args)
    },
    listFiles: async (args) => (await resolveClient()).listFiles(args),
    listMarkdownDocuments: async (args) => (await resolveClient()).listMarkdownDocuments(args),
    importExternalPaths: async (args) => (await resolveClient()).importExternalPaths(args),
    stageExternalPathsForRuntimeUpload: async (args) =>
      (await resolveClient()).stageExternalPathsForRuntimeUpload(args),
    resolveDroppedPathsForAgent: async (args) =>
      (await resolveClient()).resolveDroppedPathsForAgent(args),
    downloadFile: async (args) => (await resolveClient()).downloadFile(args),
    downloadFolder: async (args) => (await resolveClient()).downloadFolder(args),
    saveDownloadedFile: async (args) => (await resolveClient()).saveDownloadedFile(args),
    startDownloadedFile: async (args) => (await resolveClient()).startDownloadedFile(args),
    appendDownloadedFileChunk: async (args) =>
      (await resolveClient()).appendDownloadedFileChunk(args),
    finishDownloadedFile: async (args) => (await resolveClient()).finishDownloadedFile(args),
    cancelDownloadedFile: async (args) => (await resolveClient()).cancelDownloadedFile(args),
    readLocalLogTail: async (args) => (await resolveClient()).readLocalLogTail(args),
    startLocalLogTail: async (args) => {
      await (await resolveClient()).startLocalLogTail(args)
    },
    stopLocalLogTail: async (args) => {
      await (await resolveClient()).stopLocalLogTail(args)
    },
    onLocalLogTailChanged: async (callback) =>
      (await resolveClient()).onLocalLogTailChanged(callback),
    onFsChanged: (listener) =>
      syncUnsubscribeFsChanged(async () => (await resolveClient()).onFsChanged(listener))
  }
}
