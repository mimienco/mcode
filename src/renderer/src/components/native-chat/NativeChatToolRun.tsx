import { useEffect, useId, useState } from 'react'
import { ChevronRight } from 'lucide-react'
import { cn } from '@/lib/utils'
import { translate } from '@/i18n/i18n'
import {
  isToolCallBlock,
  isToolResultBlock,
  type NativeChatBlock
} from '../../../../shared/native-chat-types'
import { diffFromText, diffFromToolCall, type DiffLine } from './native-chat-diff'
import {
  countToolCalls,
  createToolInputDisplay,
  MAX_TOOL_DETAIL_LENGTH,
  summarizeToolRun,
  truncateToolDetail
} from './native-chat-tool-summary'
import { NativeChatDiffView } from './NativeChatDiffView'
import { NativeChatCopyButton } from './NativeChatCopyButton'

/** A single tool line that expands in place to show its diff, input, or result.
 *  Lines only mount while the parent run is open, so each starts expanded
 *  (opening the run reveals every line at once) and remains individually
 *  collapsible. */
function ToolLine({ block }: { block: NativeChatBlock }): React.JSX.Element | null {
  const [expanded, setExpanded] = useState(true)
  const detailId = useId()

  let name: string
  let preview: string
  let diff: DiffLine[] | null = null
  let body: { output: string; isError?: boolean } | null = null
  let detail: string | null = null
  let inputHasDetail = false

  if (isToolCallBlock(block)) {
    name = block.name
    const inputDisplay = createToolInputDisplay(block.input)
    preview = inputDisplay.label
    inputHasDetail = inputDisplay.hasDetail
    diff = expanded ? diffFromToolCall(block.name, block.input) : null
    detail = expanded && !diff ? inputDisplay.formatDetail() : null
  } else if (isToolResultBlock(block)) {
    name = translate('components.native-chat.tool.result', 'Result')
    preview = block.output.split('\n')[0]?.slice(0, 80) ?? ''
    diff = expanded ? diffFromText(block.output) : null
    body = { output: block.output, isError: block.isError }
  } else {
    return null
  }

  const hasDetail = diff !== null || body !== null || inputHasDetail

  return (
    <div className="min-w-0">
      <button
        type="button"
        onClick={() => hasDetail && setExpanded((v) => !v)}
        aria-expanded={hasDetail ? expanded : undefined}
        aria-controls={hasDetail ? detailId : undefined}
        className={cn(
          'group flex min-h-7 w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left transition-colors hover:bg-background/55',
          hasDetail ? 'cursor-pointer' : 'cursor-default'
        )}
      >
        {hasDetail ? (
          <ChevronRight
            className={cn(
              'size-3 shrink-0 text-muted-foreground/70 transition-transform',
              expanded && 'rotate-90'
            )}
          />
        ) : (
          <span className="size-3 shrink-0" aria-hidden="true" />
        )}
        <code className="shrink-0 font-mono text-xs font-semibold text-foreground/90 transition-colors group-hover:text-foreground">
          {name}
        </code>
        {preview ? (
          <span
            className="min-w-0 truncate font-mono text-[11px] text-muted-foreground transition-colors group-hover:text-foreground/70"
            title={preview}
          >
            {preview}
          </span>
        ) : null}
      </button>
      {hasDetail && expanded ? (
        <div id={detailId} className="space-y-1.5 py-1 pr-1 pl-6">
          {diff ? <NativeChatDiffView lines={diff} /> : null}
          {!diff && body ? (
            <>
              <pre
                className={cn(
                  'max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background/55 p-2 font-mono text-[11px] scrollbar-sleek',
                  body.isError ? 'text-destructive' : 'text-foreground/80'
                )}
              >
                {truncateToolDetail(body.output)}
              </pre>
              {body.output.length > MAX_TOOL_DETAIL_LENGTH ? (
                <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
                  <span>
                    {translate(
                      'components.native-chat.tool.truncated',
                      'Showing the first {{value0}} characters',
                      { value0: MAX_TOOL_DETAIL_LENGTH }
                    )}
                  </span>
                  <NativeChatCopyButton
                    text={body.output}
                    label={translate(
                      'components.native-chat.tool.copyFullOutput',
                      'Copy full output'
                    )}
                  />
                </div>
              ) : null}
            </>
          ) : null}
          {!diff && !body && detail ? (
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background/55 p-2 font-mono text-[11px] text-foreground/80 scrollbar-sleek">
              {detail}
            </pre>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

/** A run of a message's tool calls/results, collapsed to a one-line summary that
 *  expands to the individual inline tool lines. `expandSignal` lets the global
 *  toolbar toggle drive every run at once while still allowing per-run override. */
export function NativeChatToolRun({
  blocks,
  expandSignal
}: {
  blocks: NativeChatBlock[]
  /** Toolbar-driven desired open state. Each change re-syncs this run's state. */
  expandSignal: boolean
}): React.JSX.Element {
  const [open, setOpen] = useState(expandSignal)
  const contentId = useId()
  // Re-sync when the global toolbar toggle flips.
  useEffect(() => setOpen(expandSignal), [expandSignal])

  const callCount = countToolCalls(blocks) || blocks.length
  const summary = summarizeToolRun(blocks)
  const fallbackLabel =
    callCount === 1
      ? translate('components.native-chat.tool.countOne', '1 tool call')
      : translate('components.native-chat.tool.countN', '{{value0}} tool calls', {
          value0: callCount
        })

  return (
    <div
      data-native-chat-tool-run
      className="mt-3 overflow-hidden rounded-lg border border-border/60 bg-muted/25"
    >
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-controls={contentId}
        className="group flex min-h-9 w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-muted/45"
      >
        <span className="readout shrink-0 text-[10px] font-semibold text-muted-foreground transition-colors group-hover:text-foreground/80">
          {callCount}×
        </span>
        <span className="min-w-0 truncate font-mono text-[11px] text-muted-foreground transition-colors group-hover:text-foreground/80">
          {summary || fallbackLabel}
        </span>
        {/* Chevron on the right, revealed on hover when collapsed and pointing
            down when open — matches Codex's tool-run disclosure. */}
        <ChevronRight
          className={cn(
            'ml-auto size-3.5 shrink-0 text-muted-foreground/70 transition-transform',
            open && 'rotate-90'
          )}
        />
      </button>
      {open ? (
        <div id={contentId} className="border-t border-border/50 px-2 py-2">
          {blocks.map((block, i) => (
            <ToolLine key={i} block={block} />
          ))}
        </div>
      ) : null}
    </div>
  )
}
