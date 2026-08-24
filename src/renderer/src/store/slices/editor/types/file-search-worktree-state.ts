import type { SearchResult } from '../../../../../../shared/code-search-types'
import type { FileSearchResultOwner } from '@/lib/file-search-result-owner'

export type FileSearchWorktreeState = {
  query: string
  caseSensitive: boolean
  wholeWord: boolean
  useRegex: boolean
  includePattern: string
  excludePattern: string
  results: SearchResult | null
  resultOwner: FileSearchResultOwner | null
  loading: boolean
  collapsedFiles: Set<string>
  replaceOpen: boolean
  replaceValue: string
  preserveCase: 'none' | 'match'
  seedRequestId?: number
  focusRequestId?: number
}
