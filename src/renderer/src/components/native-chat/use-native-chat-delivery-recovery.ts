import { useCallback, type RefObject } from 'react'
import { translate } from '@/i18n/i18n'

export type NativeChatDeliveryRecovery = (
  delivery: Promise<boolean> | undefined,
  pendingId: string | undefined,
  draft: string,
  imagePaths: string[],
  filePaths?: string[]
) => void

export function useNativeChatDeliveryRecovery({
  onOptimisticSendCanceled,
  setDraft,
  attachResolvedPaths,
  setNotice,
  textareaRef
}: {
  onOptimisticSendCanceled?: (pendingId: string) => void
  setDraft: (next: string | ((current: string) => string)) => void
  attachResolvedPaths: (paths: string[]) => void
  setNotice: (notice: string | null) => void
  textareaRef: RefObject<HTMLTextAreaElement | null>
}): NativeChatDeliveryRecovery {
  return useCallback(
    (delivery, pendingId, draft, imagePaths, filePaths = []) => {
      void delivery?.then((delivered) => {
        if (delivered) {
          return
        }
        if (pendingId) {
          onOptimisticSendCanceled?.(pendingId)
        }
        setDraft((current) => {
          if (draft.length === 0) {
            return current
          }
          return current.length > 0 ? `${draft}\n\n${current}` : draft
        })
        const attachmentPaths = [...filePaths, ...imagePaths]
        if (attachmentPaths.length > 0) {
          attachResolvedPaths(attachmentPaths)
        }
        setNotice(
          translate(
            'components.native-chat.composer.deliveryFailed',
            'Message not sent. Your draft was restored; try again.'
          )
        )
        requestAnimationFrame(() => textareaRef.current?.focus())
      })
    },
    [attachResolvedPaths, onOptimisticSendCanceled, setDraft, setNotice, textareaRef]
  )
}
