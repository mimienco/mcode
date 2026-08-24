// ─── Search & Replace ───────────────────────────────────
// Wire types for bulk replace over search results. Shared by the local main
// process, the relay fs bridge, and the renderer.

import type { SearchFileResult } from './code-search-types'

/** How replacement text adapts to the casing of the text it replaces. */
export type PreserveCaseMode = 'none' | 'match'

export type ReplaceOptions = {
  caseSensitive?: boolean
  wholeWord?: boolean
  useRegex?: boolean
  /** Adapt replacement casing to each matched span. Ignored with `useRegex`. */
  preserveCase?: PreserveCaseMode
}

export type ReplaceRequest = {
  rootPath: string
  query: string
  replacement: string
  options: ReplaceOptions
  /** Exactly the files the active search surfaced; replace never scans itself. */
  targets: Pick<SearchFileResult, 'filePath' | 'relativePath'>[]
}

export type ReplaceFileOutcomeStatus = 'written' | 'skipped' | 'failed'

export type ReplaceFileOutcome = {
  relativePath: string
  status: ReplaceFileOutcomeStatus
  /** Occurrences rewritten; present when status is 'written'. */
  occurrences?: number
  /** Localized-free reason code; UI maps codes to translated copy. */
  reason?: 'unchanged' | 'dirty-editor-tab' | 'binary-file' | 'too-large' | 'read-error' | 'write-error' | 'outside-root' | 'gone'
}

export type ReplaceSummary = {
  outcomes: ReplaceFileOutcome[]
  writtenFiles: number
  writtenOccurrences: number
  skippedFiles: number
  failedFiles: number
}

export type ReplaceProgressEvent = {
  requestId: string
  done: number
  total: number
  last?: ReplaceFileOutcome
}
