import { X } from 'lucide-react'
import { translate } from '@/i18n/i18n'
import { getFileTypeIcon, getFileTypeIconColorClass } from '@/lib/file-type-icons'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { NativeChatComposerFileReference } from './use-native-chat-composer-attachments'

export function NativeChatFileReferenceChip({
  reference,
  onRemove
}: {
  reference: NativeChatComposerFileReference
  onRemove: (id: string) => void
}): React.JSX.Element {
  const FileIcon = getFileTypeIcon(reference.path)
  return (
    <div
      data-native-chat-file-reference
      className="flex h-8 max-w-full items-center gap-1.5 rounded-md border border-border bg-background pl-2 pr-1 text-xs text-muted-foreground"
    >
      <FileIcon
        aria-hidden="true"
        className={cn(
          'size-3.5 shrink-0 text-muted-foreground',
          getFileTypeIconColorClass(reference.path)
        )}
      />
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            tabIndex={0}
            className="max-w-64 truncate rounded-sm font-mono text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            @{reference.path}
          </span>
        </TooltipTrigger>
        <TooltipContent side="top" sideOffset={4} className="max-w-96 font-mono break-all">
          {reference.path}
        </TooltipContent>
      </Tooltip>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        onClick={() => onRemove(reference.id)}
        aria-label={translate(
          'components.native-chat.composer.removeAttachment',
          'Remove attachment'
        )}
        className="text-muted-foreground"
      >
        <X className="size-3" />
      </Button>
    </div>
  )
}
