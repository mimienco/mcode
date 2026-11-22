import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { ArrowDown, ArrowUp } from 'lucide-react'
import CommentMarkdown, {
  type CommentMarkdownLinkClickHandler
} from '@/components/sidebar/CommentMarkdown'
import { cn } from '@/lib/utils'
import { translate } from '@/i18n/i18n'
import {
  isTextBlock,
  type NativeChatBlock,
  type NativeChatMessage
} from '../../../../shared/native-chat-types'
import type { NativeChatLiveSession } from './use-native-chat-live-session'
import { orderNativeChatMessages } from './native-chat-message-grouping'
import { stripNoiseMessages } from './native-chat-noise'
import { createNativeChatToolFoldCache, splitNativeChatBlocks } from './native-chat-tool-fold'
import { ShowEarlierInWindowButton, useWindowedTranscript } from './use-windowed-transcript'
import { isNearBottom, shouldShowJumpToLatest, type ScrollGeometry } from './native-chat-autoscroll'
import { splitNativeChatSessionContext } from './native-chat-context-disclosure'
import { NativeChatSessionContextDisclosure } from './NativeChatSessionContextDisclosure'
import { NativeChatToolRun } from './NativeChatToolRun'
import { NativeChatCopyButton } from './NativeChatCopyButton'
import { NativeChatImageRefs } from './NativeChatImageRefs'
import { NativeChatUserMessageImageChips } from './NativeChatUserMessageImageChips'
import { extractUserMessageImagePaths } from './native-chat-user-message-image-paths'
import { NATIVE_CHAT_STREAMING_ID } from '../../../../shared/native-chat-streaming'

function geometryOf(el: HTMLElement): ScrollGeometry {
  return { scrollTop: el.scrollTop, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight }
}

function proseToMarkdown(blocks: NativeChatBlock[]): string {
  return blocks
    .map((block) => {
      if (isTextBlock(block)) {
        return block.text
      }
      return ''
    })
    .filter((part) => part.length > 0)
    .join('\n\n')
}

/** A quiet message footer: actions stay attached to the prose they control
 *  instead of floating in the transcript gutter. */
function AgentControls({
  markdown,
  onScrollToTop
}: {
  markdown: string
  onScrollToTop: () => void
}): React.JSX.Element {
  return (
    <div
      data-native-chat-message-actions
      role="toolbar"
      aria-label={translate('components.native-chat.messageActions', 'Message actions')}
      className="mt-2 flex items-center justify-end gap-0.5 border-t border-border/40 pt-1.5 opacity-70 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
    >
      <NativeChatCopyButton text={markdown} className="size-7" />
      <button
        type="button"
        onClick={onScrollToTop}
        aria-label={translate(
          'components.native-chat.scrollMessageToTop',
          'Scroll this message to top'
        )}
        title={translate('components.native-chat.scrollMessageToTop', 'Scroll this message to top')}
        className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <ArrowUp className="size-3.5" />
      </button>
    </div>
  )
}

function TypingIndicatorRow(): React.JSX.Element {
  return (
    <div
      className="flex items-center justify-start"
      aria-label={translate('components.native-chat.status.responding', 'Agent is responding')}
      aria-live="polite"
    >
      <div className="flex h-8 items-center gap-1.5 text-muted-foreground">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="native-chat-typing-dot size-1.5 rounded-full bg-muted-foreground/70"
            // Stagger the three dots so they ripple rather than pulse in unison.
            style={{ animationDelay: `${i * 160}ms` }}
          />
        ))}
      </div>
    </div>
  )
}

/** One message: its prose first, then a collapsible run folding all of the
 *  turn's tool activity. Monochrome per STYLEGUIDE: user prompts read as a
 *  lifted card, assistant prose as body copy, reasoning de-emphasized. */
const MessageRow = memo(function MessageRow({
  message,
  previousRole,
  expandSignal,
  onScrollMessageToTop,
  onLinkClick,
  allowFileUriLinks = false,
  deliveryFailed = false
}: {
  message: NativeChatMessage
  previousRole?: NativeChatMessage['role']
  expandSignal: boolean
  /** Align this message's top to the top of the scroll viewport. */
  onScrollMessageToTop: (el: HTMLElement) => void
  onLinkClick?: CommentMarkdownLinkClickHandler
  allowFileUriLinks?: boolean
  deliveryFailed?: boolean
}): React.JSX.Element | null {
  const rowRef = useRef<HTMLDivElement | null>(null)
  const { prose, tools } = useMemo(() => splitNativeChatBlocks(message.blocks), [message.blocks])
  // Why: PERF-07 — prose concatenation must not rerun for every message on
  // every parent render (stream appends re-render the list each frame).
  const markdown = useMemo(() => proseToMarkdown(prose), [prose])
  const isUser = message.role === 'user'
  const isPending = message.id.startsWith('pending:')
  const isReasoning = message.role === 'reasoning'
  const isSystem = message.role === 'system'
  const spacingClass =
    previousRole === undefined
      ? undefined
      : isUser
        ? 'mt-8'
        : previousRole === 'user'
          ? 'mt-6 sm:mt-3'
          : 'mt-5'
  const disclosedUserContent = useMemo(
    () => splitNativeChatSessionContext(isUser ? markdown : ''),
    [isUser, markdown]
  )
  const userMessageImages = useMemo(
    () => extractUserMessageImagePaths(disclosedUserContent.visibleText),
    [disclosedUserContent.visibleText]
  )
  const hasImages = prose.some((block) => block.type === 'image-ref')

  const scrollToTop = useCallback(() => {
    if (rowRef.current) {
      onScrollMessageToTop(rowRef.current)
    }
  }, [onScrollMessageToTop])

  // Skip rows with nothing renderable so the transcript shows no empty/ghost
  // bubble.
  // After all hooks, so hook order stays unconditional.
  if (markdown.length === 0 && !hasImages && tools.length === 0) {
    return null
  }

  if (isUser) {
    // Keep the optimistic bubble visually stable while the transcript catches
    // up, but state delivery honestly in the small status line below it.
    return (
      <div
        ref={rowRef}
        data-native-chat-role="user"
        className={cn('flex flex-col items-end gap-1', spacingClass)}
      >
        {/* User turns get a distinct muted fill (not the card/canvas color) so
            the prompt reads apart from the assistant's body copy. */}
        <div className="max-w-[min(42rem,88%)] rounded-xl rounded-br-sm border border-border/70 bg-muted/60 px-4 py-3 text-sm leading-relaxed text-foreground">
          <NativeChatUserMessageImageChips imagePaths={userMessageImages.imagePaths} />
          <NativeChatImageRefs blocks={prose} showPreviews={false} />
          {userMessageImages.text ? (
            <CommentMarkdown
              content={userMessageImages.text}
              variant="document"
              className="text-sm"
              onLinkClick={onLinkClick}
              allowFileUriLinks={allowFileUriLinks}
            />
          ) : null}
          <NativeChatSessionContextDisclosure disclosure={disclosedUserContent} />
        </div>
        {deliveryFailed ? (
          <div role="alert" className="max-w-[min(42rem,88%)] px-1 text-xs text-destructive">
            {translate(
              'components.native-chat.launchPromptNotDelivered',
              'Not delivered — check the terminal'
            )}
          </div>
        ) : isPending ? (
          <div
            role="status"
            className="max-w-[min(42rem,88%)] px-1 text-[11px] text-muted-foreground"
          >
            {translate('components.native-chat.message.sending', 'Sending…')}
          </div>
        ) : null}
      </div>
    )
  }

  if (isReasoning) {
    return (
      <div
        ref={rowRef}
        data-native-chat-role="reasoning"
        className={cn('max-w-[72ch] text-sm text-muted-foreground', spacingClass)}
      >
        <details className="group/reasoning border-l border-border/70 pl-3">
          <summary className="cursor-pointer select-none py-1 text-xs font-medium not-italic hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
            {translate('components.native-chat.reasoningDisclosure', 'Reasoning')}
          </summary>
          <div className="pt-1 italic leading-relaxed">
            <NativeChatImageRefs blocks={prose} showPreviews />
            {markdown ? (
              <CommentMarkdown
                content={markdown}
                variant="document"
                className="text-sm"
                onLinkClick={onLinkClick}
                allowFileUriLinks={allowFileUriLinks}
              />
            ) : null}
            {tools.length > 0 ? (
              <NativeChatToolRun blocks={tools} expandSignal={expandSignal} />
            ) : null}
          </div>
        </details>
      </div>
    )
  }

  // Plain assistant prose is the copyable unit; reasoning/system asides stay
  // chrome-free. The controls live in a small footer rail below the message.
  const showControls = !isReasoning && !isSystem && markdown.length > 0

  return (
    <div
      ref={rowRef}
      data-native-chat-role={message.role}
      className={cn(
        'group max-w-[72ch] text-sm leading-relaxed text-foreground',
        spacingClass,
        isSystem && 'text-xs text-muted-foreground'
      )}
    >
      <NativeChatImageRefs blocks={prose} showPreviews />
      {markdown ? (
        <CommentMarkdown
          content={markdown}
          variant="document"
          className="text-sm"
          onLinkClick={onLinkClick}
          allowFileUriLinks={allowFileUriLinks}
        />
      ) : null}
      {tools.length > 0 ? <NativeChatToolRun blocks={tools} expandSignal={expandSignal} /> : null}
      {showControls ? <AgentControls markdown={markdown} onScrollToTop={scrollToTop} /> : null}
    </div>
  )
})

export function NativeChatMessageList({
  session,
  isWorking,
  expandSignal,
  fontScale,
  onLinkClick,
  allowFileUriLinks = false,
  failedDeliveryMessageIds
}: {
  session: NativeChatLiveSession
  isWorking: boolean
  /** Toolbar-driven desired open state for every tool run; each flip re-syncs. */
  expandSignal: boolean
  /** Chat-only text multiplier (1 = default), driven by the zoom shortcuts. */
  fontScale: number
  onLinkClick?: CommentMarkdownLinkClickHandler
  allowFileUriLinks?: boolean
  failedDeliveryMessageIds?: ReadonlySet<string>
}): React.JSX.Element {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const contentRef = useRef<HTMLDivElement | null>(null)
  const [stuckToBottom, setStuckToBottom] = useState(true)
  const [showJump, setShowJump] = useState(false)

  // Why: mirror stuck state into a ref so the auto-scroll layout effect can read
  // it without depending on it — depending on stuckToBottom (which scrollToBottom
  // sets) would re-fire the effect in a self-loop.
  const stuckToBottomRef = useRef(stuckToBottom)
  stuckToBottomRef.current = stuckToBottom

  const { hasMore, loadingEarlier, loadEarlier } = session

  // Strip harness noise (task-notifications, system reminders, slash-command
  // envelopes) before folding so they don't render as the user's own bubbles —
  // matching the mobile chat. Then fold each turn's tool activity into the
  // assistant message it belongs to, ordered stably, so a turn's tools collapse
  // under one run.
  // Why: PERF-08 — the fold runs on every stream tick; caching folded output
  // per source run keeps unchanged rows' identities stable so MessageRow's
  // memo holds and only the growing tail produces fresh objects.
  const foldCache = useMemo(createNativeChatToolFoldCache, [])
  const messages = useMemo(
    () => foldCache.fold(orderNativeChatMessages(stripNoiseMessages(session.messages))),
    [foldCache, session.messages]
  )
  // Why: PERF-07 — bounded tail window keeps per-frame DOM diffs small.
  const { visibleMessages, hiddenCount, showEarlierInWindow } = useWindowedTranscript(
    messages,
    scrollRef
  )
  const showTypingIndicator =
    isWorking && !messages.some((message) => message.id === NATIVE_CHAT_STREAMING_ID)

  // When an older page prepends, the scroll content grows above the viewport.
  // Capture the pre-render scroll height so the layout effect can restore the
  // user's position (no jump) instead of letting the browser keep scrollTop.
  const prependAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)

  const handleScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) {
      return
    }
    const geometry = geometryOf(el)
    const stick = isNearBottom(geometry)
    setStuckToBottom(stick)
    setShowJump(shouldShowJumpToLatest(stick, geometry))
    // Near the top — page in older history, anchoring the current position so the
    // prepend doesn't yank the view.
    if (geometry.scrollTop < 80 && hasMore && !loadingEarlier) {
      prependAnchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop }
      loadEarlier()
    }
  }, [hasMore, loadingEarlier, loadEarlier])

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current
    if (!el) {
      return
    }
    el.scrollTop = el.scrollHeight
    setStuckToBottom(true)
    setShowJump(false)
  }, [])

  // Align a single message's top to the top of the scroll viewport.
  const scrollMessageToTop = useCallback((el: HTMLElement) => {
    const container = scrollRef.current
    if (!container) {
      return
    }
    // Detach synchronously (not just via the pending onScroll) so an in-place
    // streaming growth can't re-pin to the bottom mid-flight and fight this
    // deliberate scroll. The ref is what the resize observer reads.
    stuckToBottomRef.current = false
    setStuckToBottom(false)
    const delta = el.getBoundingClientRect().top - container.getBoundingClientRect().top
    container.scrollTo({ top: container.scrollTop + delta, behavior: 'smooth' })
  }, [])

  // Re-pin to the bottom when new content arrives, but only if the user hasn't
  // scrolled up. Layout effect so the jump happens before paint (no flicker).
  // When an older page just prepended, restore the prior position instead.
  useLayoutEffect(() => {
    const el = scrollRef.current
    if (el && prependAnchorRef.current) {
      // Preserve the viewport: shift scrollTop by however much taller the content
      // got, so the message the user was reading stays put.
      const grew = el.scrollHeight - prependAnchorRef.current.scrollHeight
      el.scrollTop = prependAnchorRef.current.scrollTop + grew
      prependAnchorRef.current = null
      return
    }
    if (stuckToBottomRef.current) {
      scrollToBottom()
    }
  }, [messages.length, isWorking, showTypingIndicator, scrollToBottom])

  // Content growing without a message-count change (a streaming assistant turn
  // extends its own message in place) never re-fires the layout effect above.
  // Observe the container so those in-place growths still re-pin: stay glued to
  // the bottom while stuck, otherwise just refresh the jump affordance. This is
  // what removes most "Jump to latest" clicks during a live response.
  useEffect(() => {
    const el = scrollRef.current
    if (!el || typeof ResizeObserver === 'undefined') {
      return
    }
    const observer = new ResizeObserver(() => {
      if (stuckToBottomRef.current) {
        scrollToBottom()
      } else {
        handleScroll()
      }
    })
    // Observe the growing content, not just the fixed-height viewport, so an
    // in-place streaming growth is seen; also watch the viewport for reflows.
    observer.observe(el)
    if (contentRef.current) {
      observer.observe(contentRef.current)
    }
    return () => observer.disconnect()
  }, [handleScroll, scrollToBottom])

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="scrollbar-sleek h-full overflow-y-auto px-3 pt-8 pb-6 sm:px-5"
      >
        <div
          ref={contentRef}
          data-native-chat-transcript
          role="log"
          aria-label={translate('components.native-chat.transcript.label', 'Conversation')}
          aria-live="polite"
          aria-relevant="additions text"
          aria-busy={isWorking}
          // Why: 3xl is the default reading column; 4xl when the pane itself
          // is wide (container, not viewport) so split views stay compact.
          className="mx-auto flex w-full max-w-3xl flex-col @5xl/native-chat:max-w-4xl"
          // Why: `zoom` scales the chat transcript's text and layout together,
          // scoped to this container so the rest of the app is untouched. It's
          // the desktop analog of the mobile pinch-zoom (Chromium/Electron only).
          style={{ zoom: fontScale }}
        >
          {hasMore ? (
            <div className="flex justify-center py-1">
              <button
                type="button"
                onClick={loadEarlier}
                disabled={loadingEarlier}
                className="min-h-8 rounded-md px-3 py-1 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
              >
                {loadingEarlier
                  ? translate('components.native-chat.loadingEarlier', 'Loading…')
                  : translate('components.native-chat.loadEarlier', 'Load earlier messages')}
              </button>
            </div>
          ) : null}
          <ShowEarlierInWindowButton count={hiddenCount} onClick={showEarlierInWindow} />
          {visibleMessages.map((message, index) => {
            const absoluteIndex = messages.length - visibleMessages.length + index
            return (
              <MessageRow
                key={message.id}
                message={message}
                previousRole={
                  index > 0 ? visibleMessages[index - 1].role : messages[absoluteIndex - 1]?.role
                }
                expandSignal={expandSignal}
                onScrollMessageToTop={scrollMessageToTop}
                onLinkClick={onLinkClick}
                allowFileUriLinks={allowFileUriLinks}
                deliveryFailed={failedDeliveryMessageIds?.has(message.id) === true}
              />
            )
          })}
          {showTypingIndicator ? <TypingIndicatorRow /> : null}
        </div>
      </div>
      {showJump ? (
        <button
          type="button"
          onClick={scrollToBottom}
          aria-label={translate('components.native-chat.jumpToLatest', 'Jump to latest')}
          className="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-border bg-card/90 px-3 py-1.5 text-xs text-muted-foreground shadow-sm backdrop-blur hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ArrowDown className="size-3.5" />
          <span>{translate('components.native-chat.jumpToLatest', 'Jump to latest')}</span>
        </button>
      ) : null}
    </div>
  )
}
