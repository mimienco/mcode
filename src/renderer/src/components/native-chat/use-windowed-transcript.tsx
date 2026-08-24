import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { NativeChatMessage } from '../../../../shared/native-chat-types'

// Why: PERF-07 — rendering the full history DOM-diffs thousands of rows per
// stream frame. Render a bounded tail window; older rows expand on demand
// while preserving the reader's viewport across the prepend.

const RENDER_WINDOW_STEP = 400

export type WindowedTranscript = {
  visibleMessages: readonly NativeChatMessage[]
  hiddenCount: number
  showEarlierInWindow: () => void
}

export function useWindowedTranscript(
  messages: readonly NativeChatMessage[],
  scrollRef: React.MutableRefObject<HTMLDivElement | null>
): WindowedTranscript {
  const [visibleWindow, setVisibleWindow] = useState(RENDER_WINDOW_STEP)
  const expandAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)

  const hiddenCount = Math.max(0, messages.length - visibleWindow)
  const visibleMessages = useMemo(
    () => (hiddenCount > 0 ? messages.slice(-visibleWindow) : messages),
    [messages, hiddenCount, visibleWindow]
  )

  // Why: capture before expanding so the reader's position survives the prepend,
  // mirroring how loadEarlier restores its prepended page.
  const showEarlierInWindow = useCallback((): void => {
    const el = scrollRef.current
    if (el) {
      expandAnchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop }
    }
    setVisibleWindow((current) => current + RENDER_WINDOW_STEP)
  }, [scrollRef])

  // Why: expanding the window prepends rows; restore the viewport so the
  // message being read stays put instead of jumping.
  useLayoutEffect(() => {
    const el = scrollRef.current
    if (!el || !expandAnchorRef.current) {
      return
    }
    const grew = el.scrollHeight - expandAnchorRef.current.scrollHeight
    el.scrollTop = expandAnchorRef.current.scrollTop + grew
    expandAnchorRef.current = null
  }, [scrollRef, visibleMessages.length])

  return { visibleMessages, hiddenCount, showEarlierInWindow }
}

export function ShowEarlierInWindowButton({
  count,
  onClick
}: {
  count: number
  onClick: () => void
}): React.JSX.Element {
  return (
    <div className="flex justify-center py-1">
      <button
        type="button"
        onClick={onClick}
        className="min-h-8 rounded-md px-3 py-1 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Show earlier messages ({count})
      </button>
    </div>
  )
}
