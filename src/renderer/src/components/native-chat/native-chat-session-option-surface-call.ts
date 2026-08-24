import { toast } from 'sonner'
import { translate } from '@/i18n/i18n'

export function runNativeChatSessionOptionSurfaceCall(
  pendingKey: string,
  setPendingId: (id: string | null) => void,
  call: () => Promise<unknown>
): Promise<boolean> {
  setPendingId(pendingKey)
  return call()
    .then(
      () => true,
      (error) => {
        toast.error(
          translate(
            'components.native-chat.composer.optionUpdateFailed',
            'Could not update option'
          ),
          { description: error instanceof Error ? error.message : String(error) }
        )
        return false
      }
    )
    .finally(() => setPendingId(null))
}
