// ─── Search ─────────────────────────────────────────────
export type SearchMatch = {
  line: number
  column: number
  matchLength: number
  lineContent: string
  displayColumn?: number
  displayMatchLength?: number
}

export type SearchFileResult = {
  filePath: string
  relativePath: string
  matches: SearchMatch[]
  matchCount?: number
}

export type SearchResult = {
  files: SearchFileResult[]
  totalMatches: number
  truncated: boolean
  /** Why: an empty result is indistinguishable from a missing rg binary; the UI
   *  uses this to show an install hint instead of a silent zero. */
  rgUnavailable?: boolean
}

export type SearchOptions = {
  query: string
  rootPath: string
  caseSensitive?: boolean
  wholeWord?: boolean
  useRegex?: boolean
  includePattern?: string
  excludePattern?: string
  maxResults?: number
}
