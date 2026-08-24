import { isClipboardTextByteLengthOverLimit } from './clipboard-text'
import { compareFileNames } from './file-name-sort'

export const QUICK_OPEN_RESULT_LIMIT = 50
export const QUICK_OPEN_QUERY_MAX_BYTES = 2 * 1024
export const QUICK_OPEN_REMOTE_QUERY_MAX_CODE_UNITS = 256
export const QUICK_OPEN_SEARCH_VERSION = 2

export type QuickOpenIndexedFile = {
  path: string
  lowerPath: string
  lowerFilename: string
  /** Maps each UTF-16 code unit in `lowerPath` back to its source-path index. */
  pathIndices: number[]
  inputIndex: number
}

export type QuickOpenSearchResult = {
  path: string
  score: number
  /** Indices into `path` of the characters that matched the query (empty for
   *  an empty query). Case-fold expansions are mapped back to source indices. */
  matchIndices: number[]
}

export type QuickOpenMatchMode = 'fuzzy' | 'substring'

export type QuickOpenRankOptions = {
  matchMode?: QuickOpenMatchMode
}

export function prepareQuickOpenFiles(files: readonly string[]): QuickOpenIndexedFile[] {
  return files.map((path, inputIndex) => prepareQuickOpenFile(path, inputIndex))
}

const preparedQuickOpenFiles = new WeakMap<readonly string[], QuickOpenIndexedFile[]>()

export function getPreparedQuickOpenFiles(
  files: readonly string[]
): readonly QuickOpenIndexedFile[] {
  const cached = preparedQuickOpenFiles.get(files)
  if (cached) {
    return cached
  }
  const prepared = prepareQuickOpenFiles(files)
  preparedQuickOpenFiles.set(files, prepared)
  return prepared
}

export function isQuickOpenQueryTooLarge(
  query: string,
  maxBytes = QUICK_OPEN_QUERY_MAX_BYTES
): boolean {
  return isClipboardTextByteLengthOverLimit(query, maxBytes)
}

export function isQuickOpenRemoteQueryTooLarge(query: string): boolean {
  return query.length > QUICK_OPEN_REMOTE_QUERY_MAX_CODE_UNITS || isQuickOpenQueryTooLarge(query)
}

export function rankQuickOpenFiles(
  query: string,
  files: readonly QuickOpenIndexedFile[],
  limit = QUICK_OPEN_RESULT_LIMIT,
  options: QuickOpenRankOptions = {}
): QuickOpenSearchResult[] {
  if (limit <= 0 || isQuickOpenQueryTooLarge(query)) {
    return []
  }

  const normalizedQuery = normalizeQuickOpenQuery(query)
  const results: QuickOpenRankedResult[] = []
  for (const file of files) {
    const match = normalizedQuery
      ? matchIndexedFile(normalizedQuery, file, options.matchMode ?? 'fuzzy')
      : { score: 0, indices: [] }
    if (match) {
      retainTopResult(
        results,
        {
          path: file.path,
          score: match.score,
          inputIndex: file.inputIndex,
          matchIndices: match.indices
        },
        limit
      )
    }
  }
  return finalizeResults(results)
}

export class QuickOpenPathRanker {
  private readonly normalizedQuery: string | null
  private readonly matchMode: QuickOpenMatchMode
  private readonly retained: QuickOpenRankedResult[] = []
  private inputIndex = 0
  private matchCount = 0

  constructor(
    query: string,
    private readonly limit: number,
    options: QuickOpenRankOptions = {}
  ) {
    this.matchMode = options.matchMode ?? 'fuzzy'
    this.normalizedQuery =
      limit <= 0 || isQuickOpenQueryTooLarge(query) ? null : normalizeQuickOpenQuery(query)
  }

  consider(path: string): void {
    const file = prepareQuickOpenFile(path, this.inputIndex++)
    if (this.normalizedQuery === null) {
      return
    }
    const match = this.normalizedQuery
      ? matchIndexedFile(this.normalizedQuery, file, this.matchMode)
      : { score: 0, indices: [] }
    if (!match) {
      return
    }
    this.matchCount++
    retainTopResult(
      this.retained,
      {
        path: file.path,
        score: match.score,
        inputIndex: file.inputIndex,
        matchIndices: match.indices
      },
      this.limit
    )
  }

  result(): { paths: string[]; totalCount: number } {
    return {
      paths: finalizeResults(this.retained).map((result) => result.path),
      totalCount: this.matchCount
    }
  }
}

function normalizeQuickOpenQuery(query: string): string {
  return foldSearchText(query.trim().replace(/\\/g, '/')).text
}

function prepareQuickOpenFile(path: string, inputIndex: number): QuickOpenIndexedFile {
  const searchPath = path.replace(/\\/g, '/')
  const lastSlash = searchPath.lastIndexOf('/')
  const foldedPath = foldSearchText(searchPath)
  return {
    path,
    lowerPath: foldedPath.text,
    lowerFilename: foldSearchText(searchPath.slice(lastSlash + 1)).text,
    pathIndices: foldedPath.sourceIndices,
    inputIndex
  }
}

function foldSearchText(value: string): { text: string; sourceIndices: number[] } {
  let text = ''
  const sourceIndices: number[] = []
  for (let sourceIndex = 0; sourceIndex < value.length; ) {
    const codePoint = value.codePointAt(sourceIndex)
    if (codePoint === undefined) {
      break
    }
    const sourceCharacter = String.fromCodePoint(codePoint)
    const foldedCharacter = sourceCharacter.toLowerCase()
    text += foldedCharacter
    for (let foldedIndex = 0; foldedIndex < foldedCharacter.length; foldedIndex++) {
      sourceIndices.push(sourceIndex)
    }
    sourceIndex += sourceCharacter.length
  }
  return { text, sourceIndices }
}

type QuickOpenFuzzyMatch = { score: number; indices: number[] }

function matchIndexedFile(
  query: string,
  file: QuickOpenIndexedFile,
  matchMode: QuickOpenMatchMode
): QuickOpenFuzzyMatch | null {
  return matchMode === 'substring'
    ? substringMatchIndexedFile(query, file)
    : fuzzyMatchIndexedFile(query, file)
}

function sourceMatchIndices(
  file: QuickOpenIndexedFile,
  foldedIndices: readonly number[]
): number[] {
  const sourceIndices: number[] = []
  for (const foldedIndex of foldedIndices) {
    const sourceIndex = file.pathIndices[foldedIndex]
    if (sourceIndex !== undefined && sourceIndices.at(-1) !== sourceIndex) {
      sourceIndices.push(sourceIndex)
    }
  }
  return sourceIndices
}

function substringMatchIndexedFile(
  query: string,
  file: QuickOpenIndexedFile
): QuickOpenFuzzyMatch | null {
  const filenameIndex = file.lowerFilename.indexOf(query)
  const filenameStart = file.lowerPath.length - file.lowerFilename.length
  const pathIndex =
    filenameIndex !== -1 ? filenameStart + filenameIndex : file.lowerPath.indexOf(query)
  if (pathIndex < 0) {
    return null
  }

  return {
    // Keep the incumbent filename-first ordering while requiring a literal match.
    score: filenameIndex !== -1 ? filenameIndex - 100 : pathIndex,
    indices: sourceMatchIndices(
      file,
      Array.from({ length: query.length }, (_, offset) => pathIndex + offset)
    )
  }
}

function fuzzyMatchIndexedFile(
  query: string,
  file: QuickOpenIndexedFile
): QuickOpenFuzzyMatch | null {
  let qi = 0
  let score = 0
  let lastMatchIdx = -1
  const indices: number[] = []

  for (let ti = 0; ti < file.lowerPath.length && qi < query.length; ti++) {
    if (file.lowerPath[ti] !== query[qi]) {
      continue
    }
    const gap = lastMatchIdx === -1 ? 0 : ti - lastMatchIdx - 1
    score += gap
    if (
      ti > 0 &&
      (file.lowerPath[ti - 1] === '/' ||
        file.lowerPath[ti - 1] === '.' ||
        file.lowerPath[ti - 1] === '-')
    ) {
      score -= 5
    }
    lastMatchIdx = ti
    indices.push(ti)
    qi++
  }

  if (qi < query.length) {
    return null
  }
  if (file.lowerFilename.includes(query)) {
    score -= 100
  }
  return { score, indices: sourceMatchIndices(file, indices) }
}

type QuickOpenRankedResult = QuickOpenSearchResult & {
  inputIndex: number
}

function retainTopResult(
  heap: QuickOpenRankedResult[],
  candidate: QuickOpenRankedResult,
  limit: number
): void {
  if (heap.length === limit && compareRankedResult(candidate, heap[0]) >= 0) {
    return
  }
  if (heap.length < limit) {
    heap.push(candidate)
    siftResultUp(heap, heap.length - 1)
    return
  }
  heap[0] = candidate
  siftResultDown(heap)
}

function siftResultUp(heap: QuickOpenRankedResult[], startIndex: number): void {
  let index = startIndex
  while (index > 0) {
    const parentIndex = Math.floor((index - 1) / 2)
    if (compareRankedResult(heap[index], heap[parentIndex]) <= 0) {
      return
    }
    ;[heap[index], heap[parentIndex]] = [heap[parentIndex], heap[index]]
    index = parentIndex
  }
}

function siftResultDown(heap: QuickOpenRankedResult[]): void {
  let index = 0
  while (true) {
    const leftIndex = index * 2 + 1
    if (leftIndex >= heap.length) {
      return
    }
    const rightIndex = leftIndex + 1
    const worseChildIndex =
      rightIndex < heap.length && compareRankedResult(heap[rightIndex], heap[leftIndex]) > 0
        ? rightIndex
        : leftIndex
    if (compareRankedResult(heap[worseChildIndex], heap[index]) <= 0) {
      return
    }
    ;[heap[index], heap[worseChildIndex]] = [heap[worseChildIndex], heap[index]]
    index = worseChildIndex
  }
}

function finalizeResults(results: QuickOpenRankedResult[]): QuickOpenSearchResult[] {
  return results.sort(compareRankedResult).map(({ path, score, matchIndices }) => ({
    path,
    score,
    matchIndices
  }))
}

function compareRankedResult(a: QuickOpenRankedResult, b: QuickOpenRankedResult): number {
  return a.score - b.score || compareFileNames(a.path, b.path) || a.inputIndex - b.inputIndex
}
