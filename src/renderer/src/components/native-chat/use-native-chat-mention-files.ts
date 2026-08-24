import { useCallback, useMemo, type Dispatch, type RefObject, type SetStateAction } from 'react'
import { useAppStore } from '@/store'
import { useRuntimeFileListForWorktree } from '@/components/quick-open-file-list'
import {
  getPreparedQuickOpenFiles,
  rankQuickOpenFiles,
  QUICK_OPEN_RESULT_LIMIT
} from '../../../../shared/quick-open-path-search'
import { removeActiveMentionQuery } from './native-chat-composer-state'
import { findTerminalTabWorktreeId } from './native-chat-file-link'

export const MENTION_FILE_LIMIT = QUICK_OPEN_RESULT_LIMIT

export type NativeChatMentionFile = {
  path: string
  matchIndices: number[]
}

/** `@` mention menu state for the chat composer: workspace files ranked against
 *  the live query via the quick-open pipeline (local, SSH, and remote runtimes),
 *  plus acceptance that splices the picked relative path into the draft. */
export function useNativeChatMention({
  enabled,
  terminalTabId,
  query,
  draft,
  caret,
  setDraft,
  setCaret,
  activeSuggestion,
  setActiveSuggestion,
  textareaRef,
  onChoosePath
}: {
  enabled: boolean
  terminalTabId: string
  query: string
  draft: string
  caret: number
  setDraft: (value: string) => void
  setCaret: Dispatch<SetStateAction<number>>
  activeSuggestion: number
  setActiveSuggestion: Dispatch<SetStateAction<number>>
  textareaRef: RefObject<HTMLTextAreaElement | null>
  onChoosePath: (path: string) => void
}): {
  files: NativeChatMentionFile[]
  loading: boolean
  loadError: string | null
  truncated: boolean
  accept: () => void
  choose: (path: string) => void
} {
  const worktreeId = useAppStore((state) =>
    findTerminalTabWorktreeId(state.tabsByWorktree ?? {}, terminalTabId)
  )
  const list = useRuntimeFileListForWorktree({
    enabled: enabled && worktreeId !== null,
    worktreeId,
    query,
    matchMode: 'substring',
    resultLimit: MENTION_FILE_LIMIT
  })
  const indexedFiles = useMemo(() => getPreparedQuickOpenFiles(list.files), [list.files])
  const rankedFiles = useMemo<NativeChatMentionFile[]>(() => {
    if (!enabled) {
      return []
    }
    return rankQuickOpenFiles(query, indexedFiles, MENTION_FILE_LIMIT + 1, {
      matchMode: 'substring'
    })
  }, [enabled, indexedFiles, query])
  const files = rankedFiles.slice(0, MENTION_FILE_LIMIT)

  const choose = useCallback(
    (path: string) => {
      if (!enabled) {
        return
      }
      const result = removeActiveMentionQuery(draft, caret)
      setDraft(result.draft)
      setCaret(result.caret)
      setActiveSuggestion(0)
      onChoosePath(path)
      const textarea = textareaRef.current
      textarea?.focus()
      requestAnimationFrame(() => textarea?.setSelectionRange(result.caret, result.caret))
    },
    [enabled, draft, caret, setDraft, setCaret, setActiveSuggestion, onChoosePath, textareaRef]
  )

  const accept = useCallback(() => {
    const path = files[Math.min(activeSuggestion, files.length - 1)]?.path
    if (!path) {
      return
    }
    choose(path)
  }, [files, activeSuggestion, choose])

  return {
    files,
    loading: list.loading && files.length === 0,
    loadError: list.loadError,
    truncated: list.truncated === true || rankedFiles.length > MENTION_FILE_LIMIT,
    accept,
    choose
  }
}
