import { translate } from '@/i18n/i18n'
import { basename } from '@/lib/path'
import { NativeChatImagePreviewChip } from './NativeChatImagePreviewChip'
import { isNativeChatPastedImagePath } from './native-chat-image-paste'

/** Row of preview chips for image paths extracted from a user message bubble. */
export function NativeChatUserMessageImageChips({
  imagePaths
}: {
  imagePaths: readonly string[]
}): React.JSX.Element | null {
  if (imagePaths.length === 0) {
    return null
  }
  return (
    <div className="mb-2 flex flex-wrap gap-1.5">
      {imagePaths.map((imagePath) => (
        <NativeChatImagePreviewChip
          key={imagePath}
          path={imagePath}
          label={
            isNativeChatPastedImagePath(imagePath)
              ? translate('components.native-chat.composer.pastedImageLabel', 'Pasted image')
              : basename(imagePath)
          }
          showThumbnail
        />
      ))}
    </div>
  )
}
