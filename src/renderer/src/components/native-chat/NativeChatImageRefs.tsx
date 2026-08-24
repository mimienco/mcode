import { Image as ImageIcon } from 'lucide-react'
import { ExpandableMarkdownImage } from '@/components/sidebar/MarkdownImageLightbox'
import { isTrustedCompactImageSrc } from '@/components/sidebar/comment-markdown-element-renderers'
import { translate } from '@/i18n/i18n'
import { basename } from '@/lib/path'
import type { NativeChatBlock, NativeChatImageRefBlock } from '../../../../shared/native-chat-types'
import { isNativeChatPastedImagePath } from './native-chat-image-paste'
import { NativeChatImagePreviewChip } from './NativeChatImagePreviewChip'

function imageLabel(image: NativeChatImageRefBlock): string {
  return image.alt ?? image.path ?? image.url ?? 'Image'
}

function ImageRefChip({ image }: { image: NativeChatImageRefBlock }): React.JSX.Element {
  const label = imageLabel(image)
  const name =
    image.path && isNativeChatPastedImagePath(image.path)
      ? translate('components.native-chat.composer.pastedImageLabel', 'Pasted image')
      : image.path
        ? basename(image.path)
        : label
  if (image.path) {
    // Why: local-path refs get the same anchored preview as composer chips;
    // untrusted url-only refs stay inert.
    return <NativeChatImagePreviewChip path={image.path} label={name} showThumbnail />
  }
  return (
    <div
      className="flex max-w-full items-center gap-1.5 rounded-md border border-border bg-background px-2 py-1 text-xs text-muted-foreground"
      title={label}
    >
      <ImageIcon className="size-3.5 shrink-0" />
      <span className="truncate">{name}</span>
    </div>
  )
}

function GeneratedImagePreview({ image }: { image: NativeChatImageRefBlock }): React.JSX.Element {
  return (
    <ExpandableMarkdownImage
      src={image.url!}
      alt={image.alt ?? 'Generated image'}
      triggerClassName="my-0 overflow-hidden rounded-lg"
      className="max-h-[32rem] max-w-full rounded-lg bg-muted/20 object-contain outline outline-1 outline-border/70"
    />
  )
}

export function NativeChatImageRefs({
  blocks,
  showPreviews
}: {
  blocks: NativeChatBlock[]
  showPreviews: boolean
}): React.JSX.Element | null {
  const images = blocks.filter(
    (block): block is NativeChatImageRefBlock => block.type === 'image-ref'
  )
  if (images.length === 0) {
    return null
  }
  const hasPreview = showPreviews && images.some((image) => isTrustedCompactImageSrc(image.url))
  return (
    <div className={hasPreview ? 'mb-2 grid gap-3' : 'mb-2 flex flex-wrap gap-1.5'}>
      {images.map((image, index) =>
        showPreviews && isTrustedCompactImageSrc(image.url) ? (
          <GeneratedImagePreview key={`${imageLabel(image)}-${index}`} image={image} />
        ) : (
          <ImageRefChip key={`${imageLabel(image)}-${index}`} image={image} />
        )
      )}
    </div>
  )
}
