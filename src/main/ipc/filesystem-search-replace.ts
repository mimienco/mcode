import { ipcMain } from 'electron'
import { requireSshFilesystemProvider } from '../providers/ssh-filesystem-dispatch'
import { readFile, rename, stat, unlink, writeFile } from 'node:fs/promises'
import { dirname, isAbsolute, join, relative, resolve } from 'node:path'
import { resolveAuthorizedPath } from './filesystem-auth'
import type { Store } from '../persistence'
import {
  createReplaceRegex,
  replaceFileContent,
  summarizeReplaceOutcomes
} from '../../shared/text-replace'
import { SEARCH_MAX_FILE_SIZE } from '../../shared/text-search'
import type {
  ReplaceFileOutcome,
  ReplaceOptions,
  ReplaceRequest,
  ReplaceSummary
} from '../../shared/code-replace-types'
import { createSenderScopedRequestCancellations } from './sender-scoped-request-cancellation'

const BINARY_PROBE_BYTES = 8192

type SearchReplaceArgs = ReplaceRequest & {
  connectionId?: string
  requestId?: string
  /** Absolute paths with unsaved editor tabs; never written (data-loss guard). */
  skipPaths?: string[]
}

export const REMOTE_REPLACE_UNSUPPORTED_MESSAGE =
  'Replace in files requires a newer paired MCode host. Update the remote host and reconnect.'


function isBinaryPrefix(buffer: Buffer): boolean {
  const len = Math.min(buffer.length, BINARY_PROBE_BYTES)
  for (let i = 0; i < len; i++) {
    if (buffer[i] === 0) {
      return true
    }
  }
  return false
}

/** Why: renderer targets come from search output; re-verify containment so a crafted target cannot escape the root. */
function containmentFailure(rootPath: string, relativePath: string): boolean {
  const candidate = resolve(rootPath, relativePath)
  const rel = relative(rootPath, candidate)
  return rel.startsWith('..') || isAbsolute(rel)
}

async function replaceOneFile(args: {
  rootPath: string
  relativePath: string
  regex: RegExp
  replacement: string
  options: ReplaceOptions
  skipPaths: Set<string>
}): Promise<ReplaceFileOutcome> {
  const outcomeBase = { relativePath: args.relativePath }
  if (containmentFailure(args.rootPath, args.relativePath)) {
    return { ...outcomeBase, status: 'failed', reason: 'outside-root' }
  }
  const filePath = resolve(args.rootPath, args.relativePath)
  if (args.skipPaths.has(filePath)) {
    return { ...outcomeBase, status: 'skipped', reason: 'dirty-editor-tab' }
  }
  let mode: number | undefined
  try {
    const stats = await stat(filePath)
    if (stats.size > SEARCH_MAX_FILE_SIZE) {
      return { ...outcomeBase, status: 'skipped', reason: 'too-large' }
    }
    mode = stats.mode
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return { ...outcomeBase, status: 'skipped', reason: 'gone' }
    }
    return { ...outcomeBase, status: 'failed', reason: 'read-error' }
  }

  let text: string
  try {
    const buffer = await readFile(filePath)
    if (isBinaryPrefix(buffer.subarray(0, BINARY_PROBE_BYTES))) {
      return { ...outcomeBase, status: 'skipped', reason: 'binary-file' }
    }
    text = buffer.toString('utf8')
  } catch {
    return { ...outcomeBase, status: 'failed', reason: 'read-error' }
  }

  const result = replaceFileContent(text, args.regex, args.replacement, {
    preserveCase: args.options.preserveCase
  })
  if (!result) {
    return { ...outcomeBase, status: 'skipped', reason: 'unchanged' }
  }

  // Why: temp file + rename in the same directory keeps the write atomic per file.
  const tempPath = join(
    dirname(filePath),
    `.mcode-replace-${Date.now()}-${Math.random().toString(36).slice(2)}.tmp`
  )
  try {
    await writeFile(tempPath, Buffer.from(result.content, 'utf8'), {
      mode,
      flag: 'wx'
    })
    await rename(tempPath, filePath)
  } catch {
    // Why: never leave the temp file behind; the target itself is untouched on failure.
    try {
      await unlink(tempPath)
    } catch {
      // Best-effort cleanup only.
    }
    return { ...outcomeBase, status: 'failed', reason: 'write-error' }
  }
  return { ...outcomeBase, status: 'written', occurrences: result.occurrences }
}

/**
 * Bulk replace over an existing search result set. Fails closed before touching
 * any file when the query cannot be rewritten precisely.
 */
export async function performReplace(params: {
  rootPath: string
  query: string
  replacement: string
  options: ReplaceOptions
  targets: { relativePath: string }[]
  skipPaths: Iterable<string>
  signal?: AbortSignal
}): Promise<ReplaceSummary> {
  const regex = createReplaceRegex(params.query, params.options)
  const skipPaths = new Set(Array.from(params.skipPaths, (p) => resolve(p)))
  const outcomes: ReplaceFileOutcome[] = []

  for (const target of params.targets) {
    if (params.signal?.aborted) {
      break
    }
    outcomes.push(
      await replaceOneFile({
        rootPath: params.rootPath,
        relativePath: target.relativePath,
        regex,
        replacement: params.replacement,
        options: params.options,
        skipPaths
      })
    )
  }
  return summarizeReplaceOutcomes(outcomes)
}

export function registerFilesystemSearchReplaceHandlers(store: Store): void {
  const replaceCancellations = createSenderScopedRequestCancellations()

  ipcMain.handle('fs:cancelSearchReplace', (event, args: { requestId: string }): void => {
    replaceCancellations.cancel(event, args.requestId)
  })

  ipcMain.handle(
    'fs:searchReplace',
    async (event, rawArgs: SearchReplaceArgs): Promise<ReplaceSummary> => {
      if (rawArgs.connectionId) {
        const provider = requireSshFilesystemProvider(rawArgs.connectionId)
        if (!provider.searchReplace) {
          throw new Error(REMOTE_REPLACE_UNSUPPORTED_MESSAGE)
        }
        return provider.searchReplace(rawArgs)
      }
      const rootPath = await resolveAuthorizedPath(rawArgs.rootPath, store)
      const controller = replaceCancellations.begin(event, rawArgs.requestId)
      try {
        return await performReplace({
          rootPath,
          query: rawArgs.query,
          replacement: rawArgs.replacement,
          options: rawArgs.options ?? {},
          targets: rawArgs.targets,
          skipPaths: rawArgs.skipPaths ?? [],
          signal: controller?.signal
        })
      } finally {
        replaceCancellations.finish(event, rawArgs.requestId, controller)
      }
    }
  )
}


