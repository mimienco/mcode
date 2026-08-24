import { X } from 'lucide-react'
import { translate } from '@/i18n/i18n'
import { basename } from '@/lib/path'
import { isNativeChatPastedImagePath } from './native-chat-image-paste'
import { NativeChatImagePreviewChip } from './NativeChatImagePreviewChip'

export type NativeChatImageAttachmentChipProps = {
  attachment: { id: string; path: string }
  onRemove: (id: string) => void
}

/** Composer image chip: shared preview plus a remove affordance. */
export function NativeChatImageAttachmentChip(
  props: NativeChatImageAttachmentChipProps
): React.JSX.Element {
  const { attachment, onRemove } = props
  const label = isNativeChatPastedImagePath(attachment.path)
    ? translate('components.native-chat.composer.pastedImageLabel', 'Pasted image')
    : basename(attachment.path)

  return (
    <NativeChatImagePreviewChip path={attachment.path} label={label} showThumbnail>
      <button
        type="button"
        onClick={() => onRemove(attachment.id)}
        aria-label={translate(
          'components.native-chat.composer.removeAttachment',
          'Remove attachment'
        )}
        className="flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <X className="size-3" />
      </button>
    </NativeChatImagePreviewChip>
  )
}
