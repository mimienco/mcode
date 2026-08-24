import { z } from 'zod'
import { defineMethod, defineStreamingMethod, type RpcAnyMethod } from '../core'
import { runFileWatchStream } from './file-watch-stream-lifecycle'
import { FILE_MUTATION_METHODS } from './files-mutation-methods'
import { remoteFileContentBudget } from './files-remote-content-budget'
import {
  QUICK_OPEN_RESULT_LIMIT,
  QUICK_OPEN_REMOTE_QUERY_MAX_CODE_UNITS,
  QUICK_OPEN_SEARCH_VERSION
} from '../../../../shared/quick-open-path-search'
import { limitQuickOpenSearchReplyBySerializedBytes } from '../../../../shared/quick-open-transport-budget'
import { FileOpen, WorktreeSelector } from './files-target-schemas'
import { FILE_TERMINAL_ARTIFACT_METHODS } from './files-terminal-artifact-methods'

let filesWatchSubscriptionSeq = 0

const FilePathSearch = WorktreeSelector.extend({
  query: z.string().max(QUICK_OPEN_REMOTE_QUERY_MAX_CODE_UNITS).default(''),
  limit: z.number().int().positive().max(QUICK_OPEN_RESULT_LIMIT).default(16),
  matchMode: z.enum(['fuzzy', 'substring']).optional(),
  excludePaths: z.array(z.string()).optional(),
  mode: z.literal('quick-open').optional()
})

const ResolveTerminalPath = WorktreeSelector.extend({
  pathText: z
    .unknown()
    .transform((v) => (typeof v === 'string' ? v : ''))
    .pipe(z.string().min(1, 'Missing path text')),
  terminal: z
    .unknown()
    .transform((v) => (typeof v === 'string' && v.length > 0 ? v : null))
    .nullable()
    .optional(),
  cwd: z
    .unknown()
    .transform((v) => (typeof v === 'string' && v.length > 0 ? v : null))
    .nullable()
    .optional(),
  crossWorkspace: z
    .unknown()
    .transform((v) => v === true)
    .optional(),
  nativeChatContext: z
    .object({
      tabId: z.string().min(1),
      sessionId: z.string().min(1)
    })
    .optional()
})

const FileOpenDiff = FileOpen.extend({
  staged: z.boolean().optional()
})

const FileTreePath = WorktreeSelector.extend({
  relativePath: z
    .unknown()
    .transform((v) => (typeof v === 'string' ? v : ''))
    .pipe(z.string())
})

const ServerDirectoryBrowse = z.object({
  path: z
    .unknown()
    .transform((v) => (typeof v === 'string' ? v : ''))
    .pipe(z.string())
})

const FileReadChunk = FileOpen.extend({
  offset: z.number().int().nonnegative(),
  length: z
    .number()
    .int()
    .positive()
    .max(512 * 1024)
})

const FileSearch = WorktreeSelector.extend({
  query: z.string().min(1).max(QUICK_OPEN_REMOTE_QUERY_MAX_CODE_UNITS),
  caseSensitive: z.boolean().optional(),
  wholeWord: z.boolean().optional(),
  useRegex: z.boolean().optional(),
  includePattern: z.string().max(2048).optional(),
  excludePattern: z.string().max(2048).optional(),
  maxResults: z.number().int().positive().optional()
})

const FileSearchReplace = FileSearch.omit({ maxResults: true }).extend({
  replacement: z.string().max(1024 * 1024),
  preserveCase: z.enum(['none', 'match']).optional(),
  targets: z
    .array(z.object({ relativePath: z.string().min(1) }))
    .min(1)
    .max(2000),
  skipPaths: z.array(z.string()).max(5000).optional()
})

const FileListAll = WorktreeSelector.extend({
  excludePaths: z.array(z.string()).optional()
})

const FileUnwatch = z.object({
  subscriptionId: z
    .unknown()
    .transform((value) => (typeof value === 'string' && value.length > 0 ? value : ''))
    .pipe(z.string().min(1, 'Missing subscriptionId'))
})

export const FILE_METHODS: RpcAnyMethod[] = [
  defineMethod({
    name: 'files.list',
    params: WorktreeSelector,
    handler: async (params, { runtime, signal }) =>
      signal === undefined
        ? runtime.listMobileFiles(params.worktree)
        : runtime.listMobileFiles(params.worktree, { signal })
  }),
  defineMethod({
    name: 'files.searchPaths',
    params: FilePathSearch,
    handler: async (params, { runtime, signal, clientKind, requestId }) => {
      if (params.mode !== 'quick-open') {
        return runtime.searchMobileFilePaths(params.worktree, params.query, params.limit)
      }
      const result = {
        ...(await runtime.searchQuickOpenFilePaths(
          params.worktree,
          params.query,
          params.limit,
          params.matchMode,
          params.excludePaths,
          signal
        )),
        quickOpenSearchVersion: QUICK_OPEN_SEARCH_VERSION
      }
      const maxContentBytes = remoteFileContentBudget(clientKind, requestId)
      return maxContentBytes === undefined
        ? result
        : limitQuickOpenSearchReplyBySerializedBytes(result, maxContentBytes)
    }
  }),
  defineMethod({
    name: 'files.open',
    params: FileOpen,
    handler: async (params, { runtime }) =>
      runtime.openMobileFile(params.worktree, params.relativePath)
  }),
  defineMethod({
    name: 'files.openDiff',
    params: FileOpenDiff,
    handler: async (params, { runtime }) =>
      runtime.openMobileDiff(params.worktree, params.relativePath, params.staged === true)
  }),
  defineMethod({
    name: 'files.read',
    params: FileOpen,
    handler: async (params, { runtime }) =>
      runtime.readMobileFile(params.worktree, params.relativePath)
  }),
  defineMethod({
    name: 'files.resolveTerminalPath',
    params: ResolveTerminalPath,
    handler: async (params, { runtime, clientId }) =>
      runtime.resolveTerminalPath(
        params.worktree,
        params.pathText,
        params.cwd ?? null,
        clientId,
        params.terminal ?? null,
        params.crossWorkspace === true,
        params.nativeChatContext ?? null
      )
  }),
  ...FILE_TERMINAL_ARTIFACT_METHODS,
  defineMethod({
    name: 'files.readPreview',
    params: FileOpen,
    handler: async (params, { runtime, clientKind, requestId }) => {
      const budget = remoteFileContentBudget(clientKind, requestId)
      return budget === undefined
        ? runtime.readFileExplorerPreview(params.worktree, params.relativePath)
        : runtime.readFileExplorerPreview(params.worktree, params.relativePath, budget)
    }
  }),
  defineMethod({
    name: 'files.readChunk',
    params: FileReadChunk,
    handler: async (params, { runtime }) =>
      runtime.readFileExplorerChunk(
        params.worktree,
        params.relativePath,
        params.offset,
        params.length
      )
  }),
  defineMethod({
    name: 'files.readDir',
    params: FileTreePath,
    handler: async (params, { runtime }) =>
      runtime.readFileExplorerDir(params.worktree, params.relativePath)
  }),
  defineMethod({
    name: 'files.browseServerDir',
    params: ServerDirectoryBrowse,
    handler: async (params, { runtime }) => runtime.browseServerDir(params.path)
  }),
  ...FILE_MUTATION_METHODS,
  defineMethod({
    name: 'files.search',
    params: FileSearch,
    handler: async (params, { runtime }) =>
      runtime.searchRuntimeFiles(params.worktree, {
        query: params.query,
        caseSensitive: params.caseSensitive,
        wholeWord: params.wholeWord,
        useRegex: params.useRegex,
        includePattern: params.includePattern,
        excludePattern: params.excludePattern,
        maxResults: params.maxResults
      })
  }),
  defineMethod({
    name: 'files.listAll',
    params: FileListAll,
    handler: async (params, { runtime, clientKind, requestId, signal }) => {
      const maxContentBytes = remoteFileContentBudget(clientKind, requestId)
      return runtime.listRuntimeFiles(params.worktree, {
        excludePaths: params.excludePaths,
        ...(signal === undefined ? {} : { signal }),
        ...(maxContentBytes === undefined ? {} : { maxContentBytes })
      })
    }
  }),
  defineMethod({
    name: 'files.searchReplace',
    params: FileSearchReplace,
    handler: async (params, { runtime }) =>
      runtime.searchReplaceRuntimeFiles(params.worktree, {
        query: params.query,
        replacement: params.replacement,
        options: {
          caseSensitive: params.caseSensitive,
          wholeWord: params.wholeWord,
          useRegex: params.useRegex,
          preserveCase: params.preserveCase
        },
        targets: params.targets.map((t) => ({ relativePath: t.relativePath })),
        skipPaths: params.skipPaths
      })
  }),
  defineMethod({
    name: 'files.listMarkdownDocuments',
    params: WorktreeSelector,
    handler: async (params, { runtime }) => runtime.listRuntimeMarkdownDocuments(params.worktree)
  }),
  defineMethod({
    name: 'files.stat',
    params: FileTreePath,
    handler: async (params, { runtime }) =>
      runtime.statRuntimeFile(params.worktree, params.relativePath)
  }),
  defineStreamingMethod({
    name: 'files.watch',
    params: WorktreeSelector,
    handler: async (params, { runtime, connectionId, signal }, emit) => {
      const seq = ++filesWatchSubscriptionSeq
      const subscriptionId = `files-watch-${connectionId ?? 'inproc'}-${seq}`
      await runFileWatchStream({
        runtime,
        worktree: params.worktree,
        connectionId,
        signal,
        subscriptionId,
        emit
      })
    }
  }),
  defineMethod({
    name: 'files.unwatch',
    params: FileUnwatch,
    handler: async (params, { runtime }) => {
      await runtime.cleanupSubscriptionAndWait(params.subscriptionId)
      return { unsubscribed: true }
    }
  })
]
