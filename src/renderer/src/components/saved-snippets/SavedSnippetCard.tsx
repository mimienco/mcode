import type { DragEvent } from 'react'
import { Check, Copy, FileText, Pencil, Trash2 } from 'lucide-react'
import { translate } from '@/i18n/i18n'
import { cn } from '@/lib/utils'
import { useClipboardTextCopyFeedback } from '@/hooks/use-clipboard-text-copy-feedback'
import { finishQuickCommandDrag, writeQuickCommandDragData } from '@/lib/quick-command-drag-data'
import type { SavedSnippet } from '../../../../shared/saved-snippet-types'

export function SavedSnippetCard({
  snippet,
  onEdit,
  onDelete,
  onDragStarted,
  onDragFinished
}: {
  snippet: SavedSnippet
  onEdit: () => void
  onDelete: () => void
  onDragStarted: () => void
  onDragFinished: (dropped: boolean) => void
}): React.JSX.Element {
  const { canCopy, copyText, status } = useClipboardTextCopyFeedback(snippet.text)
  const copyLabel =
    status === 'copied'
      ? translate('components.savedSnippets.card.copied', 'Copied')
      : translate('components.savedSnippets.card.copy', 'Copy {{value0}}', {
          value0: snippet.name
        })

  return (
    <div
      role="group"
      aria-label={snippet.name}
      draggable
      onDragStart={(event) => {
        if (!writeQuickCommandDragData(event.dataTransfer, snippet.name, snippet.text)) {
          event.preventDefault()
          return
        }
        onDragStarted()
      }}
      onDragEnd={(event: DragEvent<HTMLDivElement>) => {
        const handledInternally = finishQuickCommandDrag(event.clientX, event.clientY)
        onDragFinished(handledInternally || event.dataTransfer.dropEffect !== 'none')
      }}
      className="group/card cursor-grab rounded-xl border border-border/80 bg-card p-3 shadow-xs transition-[border-color,box-shadow] duration-150 hover:border-muted-foreground/40 hover:shadow-sm focus-within:border-ring/50 active:cursor-grabbing"
    >
      <div className="flex min-w-0 items-start gap-3">
        <div
          className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-ai-action-accent/10 text-ai-action-accent"
          aria-hidden="true"
        >
          <FileText className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold leading-5 text-foreground">
            {snippet.name}
          </div>
          <div className="mt-0.5 line-clamp-2 whitespace-pre-wrap text-[11px] leading-4 text-muted-foreground">
            {snippet.text}
          </div>
        </div>
        <div className="-mr-1 -mt-1 flex shrink-0 items-center gap-0.5 rounded-lg p-0.5">
          <button
            type="button"
            disabled={!canCopy}
            onClick={() => void copyText()}
            className={cn(
              'rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:opacity-40',
              status === 'copied' && 'text-status-success'
            )}
            aria-label={copyLabel}
            title={copyLabel}
          >
            {status === 'copied' ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          </button>
          <button
            type="button"
            onClick={onEdit}
            className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            aria-label={translate('components.savedSnippets.card.edit', 'Edit {{value0}}', {
              value0: snippet.name
            })}
          >
            <Pencil className="size-3.5" />
          </button>
          <button
            type="button"
            onClick={onDelete}
            className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            aria-label={translate('components.savedSnippets.card.delete', 'Delete {{value0}}', {
              value0: snippet.name
            })}
          >
            <Trash2 className="size-3.5" />
          </button>
        </div>
      </div>
    </div>
  )
}
