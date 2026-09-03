import { FS_CHANGED_EVENT, FS_LOG_TAIL_CHANGED_EVENT } from './fs-client'
import type { FsChangedPayload, FsWatchSubscription } from './fs-client'
import type {
  FsDownloadFileResult,
  FsImportItemResult,
  FsLocalLogTailChangedPayload,
  FsLocalLogTailReadArgs,
  FsLocalLogTailReadResult,
  FsLocalLogTailWatchArgs,
  FsResolveDroppedPathsResult,
  FsStagedExternalImportSource,
  FsStartDownloadedFileResult
} from './fs-extra-types'

/**
 * The Task-C remainder of the 34-method `window.api.fs` surface. Declared
 * here (not `Pick`ed off FsApi) so fs-client can compose
 * `FsApi = FsApiCore & FsExtraMethods` without a circular type reference.
 */
export type FsExtraMethods = {
  authorizeExternalPath: (args: { targetPath: string }) => Promise<void>
  listFiles: (args: {
    rootPath: string
    excludePaths?: string[]
    requestToken?: string
    maxResults?: number
    searchQuery?: string
  }) => Promise<string[]>
  listMarkdownDocuments: (args: {
    rootPath: string
  }) => Promise<{ filePath: string; relativePath: string; basename: string; name: string }[]>
  importExternalPaths: (args: {
    sourcePaths: string[]
    destDir: string
    ensureDir?: boolean
  }) => Promise<{ results: FsImportItemResult[] }>
  stageExternalPathsForRuntimeUpload: (args: { sourcePaths: string[] }) => Promise<{
    sources: FsStagedExternalImportSource[]
  }>
  resolveDroppedPathsForAgent: (args: {
    paths: string[]
    worktreePath: string
    connectionId?: string
  }) => Promise<FsResolveDroppedPathsResult>
  downloadFile: (args: { filePath: string; connectionId: string }) => Promise<FsDownloadFileResult>
  downloadFolder: (args: { dirPath: string; connectionId: string }) => Promise<FsDownloadFileResult>
  saveDownloadedFile: (args: {
    suggestedName: string
    content: string
    encoding: 'utf8' | 'base64'
  }) => Promise<FsDownloadFileResult>
  startDownloadedFile: (args: { suggestedName: string }) => Promise<FsStartDownloadedFileResult>
  appendDownloadedFileChunk: (args: {
    transferId: string
    contentBase64: string
  }) => Promise<{ ok: true }>
  finishDownloadedFile: (args: {
    transferId: string
  }) => Promise<{ canceled: false; destinationPath: string }>
  cancelDownloadedFile: (args: { transferId: string }) => Promise<{ ok: true }>
  readLocalLogTail: (args: FsLocalLogTailReadArgs) => Promise<FsLocalLogTailReadResult>
  startLocalLogTail: (args: FsLocalLogTailWatchArgs) => Promise<void>
  stopLocalLogTail: (args: { subscriptionId: string }) => Promise<void>
  onLocalLogTailChanged: (
    callback: (payload: FsLocalLogTailChangedPayload) => void
  ) => Promise<FsWatchSubscription>
}

/**
 * The Task-C remainder methods of the typed fs API (window.api.fs parity,
 * 34/34 methods). Split from fs-client.ts: the merged factory outgrew the
 * 300-line oxlint cap. Composed into the object returned by
 * `createTauriFsClient`; `call` is the same DesktopInvoke-normalized caller
 * and `listen` the same injected event source, so tests keep the seam.
 */
export function createTauriFsExtraMethods(
  call: (command: string, args?: Record<string, unknown>) => Promise<unknown>,
  listen: (event: string, listener: (payload: { payload: unknown }) => void) => Promise<() => void>
): FsExtraMethods {
  return {
    authorizeExternalPath: ({ targetPath }) =>
      call('fs_authorize_external_path', { targetPath }).then(() => undefined),
    listFiles: ({ rootPath, excludePaths, requestToken, maxResults, searchQuery }) =>
      call('fs_list_files', {
        rootPath,
        excludePaths,
        requestToken,
        maxResults,
        searchQuery
      }) as Promise<string[]>,
    listMarkdownDocuments: ({ rootPath }) =>
      call('fs_list_markdown_documents', { rootPath }) as Promise<
        { filePath: string; relativePath: string; basename: string; name: string }[]
      >,
    importExternalPaths: ({ sourcePaths, destDir, ensureDir }) =>
      call('fs_import_external_paths', { sourcePaths, destDir, ensureDir }) as Promise<{
        results: FsImportItemResult[]
      }>,
    stageExternalPathsForRuntimeUpload: ({ sourcePaths }) =>
      call('fs_stage_external_paths_for_runtime_upload', { sourcePaths }) as Promise<{
        sources: FsStagedExternalImportSource[]
      }>,
    resolveDroppedPathsForAgent: ({ paths, worktreePath, connectionId }) =>
      call('fs_resolve_dropped_paths_for_agent', {
        paths,
        worktreePath,
        connectionId
      }) as Promise<FsResolveDroppedPathsResult>,
    downloadFile: ({ filePath, connectionId }) =>
      call('fs_download_remote_file', { filePath, connectionId }) as Promise<FsDownloadFileResult>,
    downloadFolder: ({ dirPath, connectionId }) =>
      call('fs_download_remote_folder', { dirPath, connectionId }) as Promise<FsDownloadFileResult>,
    saveDownloadedFile: ({ suggestedName, content, encoding }) =>
      call('fs_save_downloaded_file', {
        suggestedName,
        content,
        encoding
      }) as Promise<FsDownloadFileResult>,
    startDownloadedFile: ({ suggestedName }) =>
      call('fs_start_download', { suggestedName }) as Promise<FsStartDownloadedFileResult>,
    appendDownloadedFileChunk: ({ transferId, contentBase64 }) =>
      call('fs_append_download_chunk', { transferId, contentBase64 }) as Promise<{ ok: true }>,
    finishDownloadedFile: ({ transferId }) =>
      call('fs_finish_download', { transferId }) as Promise<{
        canceled: false
        destinationPath: string
      }>,
    cancelDownloadedFile: ({ transferId }) =>
      call('fs_cancel_download', { transferId }) as Promise<{ ok: true }>,
    readLocalLogTail: ({ filePath, fromByteOffset, expectedIdentity }) =>
      call('fs_read_local_log_tail', {
        filePath,
        fromByteOffset,
        expectedIdentity
      }) as Promise<FsLocalLogTailReadResult>,
    startLocalLogTail: ({ filePath, subscriptionId }) =>
      call('fs_start_local_log_tail', { filePath, subscriptionId }).then(() => undefined),
    stopLocalLogTail: ({ subscriptionId }) =>
      call('fs_stop_local_log_tail', { subscriptionId }).then(() => undefined),
    async onLocalLogTailChanged(
      callback: (payload: FsLocalLogTailChangedPayload) => void
    ): Promise<FsWatchSubscription> {
      const unlisten = await listen(FS_LOG_TAIL_CHANGED_EVENT, (event) => {
        const payload = event.payload as FsLocalLogTailChangedPayload
        if (
          payload &&
          typeof payload === 'object' &&
          typeof payload.subscriptionId === 'string' &&
          (payload.eventType === 'change' || payload.eventType === 'rename')
        ) {
          callback(payload)
        }
      })
      return unlisten
    }
  }
}

/**
 * Guarded `fs://changed` subscription shared by the composed factory.
 * `listen` is the injected event source so tests keep the seam.
 */
export async function subscribeFsChanged(
  listener: (payload: FsChangedPayload) => void,
  listen: (event: string, handler: (payload: { payload: unknown }) => void) => Promise<() => void>
): Promise<FsWatchSubscription> {
  const unlisten = await listen(FS_CHANGED_EVENT, (event) => {
    if (
      event.payload &&
      typeof event.payload === 'object' &&
      'worktreePath' in event.payload &&
      Array.isArray((event.payload as FsChangedPayload).events)
    ) {
      listener(event.payload as FsChangedPayload)
    }
  })
  return unlisten
}
