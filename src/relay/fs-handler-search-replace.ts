import { readFile, rename, stat, unlink, writeFile } from 'node:fs/promises'
import { dirname, isAbsolute, join, relative, resolve } from 'node:path'
import { expandTilde } from './context'
import {
  createReplaceRegex,
  replaceFileContent,
  summarizeReplaceOutcomes
} from '../shared/text-replace'
import { SEARCH_MAX_FILE_SIZE } from '../shared/text-search'
import type {
  ReplaceFileOutcome,
  ReplaceSummary
} from '../shared/code-replace-types'

const BINARY_PROBE_BYTES = 8192

function isBinaryPrefix(buffer: Buffer): boolean {
  const len = Math.min(buffer.length, BINARY_PROBE_BYTES)
  for (let i = 0; i < len; i++) {
    if (buffer[i] === 0) {
      return true
    }
  }
  return false
}

function containmentFailure(rootPath: string, relativePath: string): boolean {
  const candidate = resolve(rootPath, relativePath)
  const rel = relative(rootPath, candidate)
  return rel.startsWith('..') || isAbsolute(rel)
}

async function replaceOneRelayFile(args: {
  rootPath: string
  relativePath: string
  regex: RegExp
  replacement: string
  preserveCase?: 'none' | 'match'
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
    preserveCase: args.preserveCase
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
 * Relay twin of the local fs:searchReplace engine. Runs on the remote host over
 * the same shared rewrite rules so SSH and headless worktrees behave identically.
 */
export async function searchReplaceOnRelay(params: Record<string, unknown>): Promise<ReplaceSummary> {
  const rootPath = expandTilde(params.rootPath as string)
  const query = params.query as string
  const replacement = params.replacement as string
  const options = (params.options ?? {}) as {
    caseSensitive?: boolean
    wholeWord?: boolean
    useRegex?: boolean
    preserveCase?: 'none' | 'match'
  }
  const targets = (params.targets ?? []) as { relativePath?: string; filePath?: string }[]
  const skipPaths = new Set(((params.skipPaths ?? []) as string[]).map((p) => resolve(expandTilde(p))))

  // Why: fail closed before touching any file — a search-highlighted span this
  // host cannot rewrite precisely must never be half-replaced.
  const regex = createReplaceRegex(query, options)

  const outcomes: ReplaceFileOutcome[] = []
  for (const target of targets) {
    const relativePath = target.relativePath
    if (typeof relativePath !== 'string' || relativePath.length === 0) {
      continue
    }
    outcomes.push(
      await replaceOneRelayFile({
        rootPath,
        relativePath,
        regex,
        replacement,
        preserveCase: options.preserveCase,
        skipPaths
      })
    )
  }
  return summarizeReplaceOutcomes(outcomes)
}
