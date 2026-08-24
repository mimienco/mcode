import { useCallback, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { useAppStore } from '@/store'
import type { SearchResult } from '../../../../shared/code-search-types'
import type { ReplaceSummary } from '../../../../shared/code-replace-types'
import { replaceRuntimeFiles } from '@/runtime/runtime-file-client'
import { translate } from '@/i18n/i18n'
import { getRightSidebarWorktreeRuntimeSettings } from './file-explorer-runtime-owner'
import { getConnectionId } from '@/lib/connection-context'

export type FileSearchReplaceModel = {
  open: boolean
  value: string
  preserveCase: 'none' | 'match'
  running: boolean
  /** Occurrence count captured when Replace All was pressed; null = not confirming. */
  confirmOccurrences: number | null
  hasResults: boolean
  onOpenToggle: () => void
  onValueChange: (value: string) => void
  onPreserveCaseToggle: () => void
  onRequestReplace: () => void
  onConfirmReplace: () => void
  onCancelConfirm: () => void
}

type UseFileSearchReplaceArgs = {
  activeWorktreeId: string | null
  worktreePath: string | null
  results: SearchResult | null
  query: string
  caseSensitive: boolean
  wholeWord: boolean
  useRegex: boolean
  rerunSearch: () => void
}

/**
 * Bulk replace over the active search results. Dirty editor tabs are never
 * written (data-loss guard, issue #7265); git is the undo layer.
 */
export function useFileSearchReplace(args: UseFileSearchReplaceArgs): FileSearchReplaceModel {
  const [running, setRunning] = useState(false)
  const [confirmOccurrences, setConfirmOccurrences] = useState<number | null>(null)

  const searchState = useAppStore((s) =>
    args.activeWorktreeId ? s.fileSearchStateByWorktree[args.activeWorktreeId] : null
  )
  const updateFileSearchState = useAppStore((s) => s.updateFileSearchState)

  const updateActiveReplaceState = useCallback(
    (updates: {
      replaceOpen?: boolean
      replaceValue?: string
      preserveCase?: 'none' | 'match'
    }) => {
      if (!args.activeWorktreeId) {
        return
      }
      updateFileSearchState(args.activeWorktreeId, updates)
    },
    [args.activeWorktreeId, updateFileSearchState]
  )

  // Why: read at run time, not render time — a tab dirtied after the panel rendered must still block its file.
  const dirtyPathsSnapshot = useCallback(
    (): string[] =>
      useAppStore
        .getState()
        .openFiles.filter(
          (file) => file.worktreeId === args.activeWorktreeId && file.isDirty && !file.isUntitled
        )
        .map((file) => file.filePath),
    [args.activeWorktreeId]
  )

  const targets = useMemo(
    () => (args.query.trim() && args.results ? args.results.files : []),
    [args.results, args.query]
  )
  const totalOccurrences = useMemo(
    () => targets.reduce((sum, file) => sum + (file.matchCount ?? file.matches.length), 0),
    [targets]
  )

  const runReplace = useCallback(async () => {
    if (!args.activeWorktreeId || !args.worktreePath || running) {
      return
    }
    setRunning(true)
    try {
      const summary = await replaceRuntimeFiles(
        {
          settings: getRightSidebarWorktreeRuntimeSettings(args.activeWorktreeId),
          worktreeId: args.activeWorktreeId,
          worktreePath: args.worktreePath,
          connectionId: getConnectionId(args.activeWorktreeId) ?? undefined
        },
        {
          query: args.query,
          replacement: searchState?.replaceValue ?? '',
          options: {
            caseSensitive: args.caseSensitive,
            wholeWord: args.wholeWord,
            useRegex: args.useRegex,
            preserveCase: searchState?.preserveCase ?? 'none'
          },
          targets,
          skipPaths: dirtyPathsSnapshot()
        }
      )
      announce(summary)
      args.rerunSearch()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      setRunning(false)
      setConfirmOccurrences(null)
    }
  }, [args, dirtyPathsSnapshot, running, searchState, targets])

  const onRequestReplace = useCallback(() => {
    if (targets.length === 0 || running || !(searchState?.replaceValue ?? '').length) {
      return
    }
    setConfirmOccurrences(totalOccurrences)
  }, [targets.length, totalOccurrences, running, searchState?.replaceValue])

  return {
    open: searchState?.replaceOpen ?? false,
    value: searchState?.replaceValue ?? '',
    preserveCase: searchState?.preserveCase ?? 'none',
    running,
    confirmOccurrences,
    hasResults: targets.length > 0,
    onOpenToggle: () =>
      updateActiveReplaceState({ replaceOpen: !(searchState?.replaceOpen ?? false) }),
    onValueChange: (value) => updateActiveReplaceState({ replaceValue: value }),
    onPreserveCaseToggle: () =>
      updateActiveReplaceState({
        preserveCase: (searchState?.preserveCase ?? 'none') === 'match' ? 'none' : 'match'
      }),
    onRequestReplace,
    onConfirmReplace: () => {
      void runReplace()
    },
    onCancelConfirm: () => setConfirmOccurrences(null)
  }
}

function announce(summary: ReplaceSummary): void {
  const skippedDirty = summary.outcomes.filter(
    (o) => o.status === 'skipped' && o.reason === 'dirty-editor-tab'
  ).length
  const parts = [
    translate(
      'auto.components.right.sidebar.replace.summary.written',
      'Replaced {{value0}} in {{value1}} file(s)',
      { value0: String(summary.writtenOccurrences), value1: String(summary.writtenFiles) }
    ),
    ...(skippedDirty > 0
      ? [
          translate(
            'auto.components.right.sidebar.replace.summary.skipped',
            '{{value0}} skipped (unsaved editor changes)',
            { value0: String(skippedDirty) }
          )
        ]
      : []),
    ...(summary.failedFiles > 0
      ? [
          translate('auto.components.right.sidebar.replace.summary.failed', '{{value0}} failed', {
            value0: String(summary.failedFiles)
          })
        ]
      : [])
  ]
  if (summary.failedFiles > 0) {
    toast.warning(parts.join(' · '))
  } else {
    toast.success(parts.join(' · '))
  }
}
