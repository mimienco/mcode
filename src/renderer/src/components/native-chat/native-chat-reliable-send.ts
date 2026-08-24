import {
  sendRuntimePtyInput,
  sendRuntimePtyInputVerified
} from '@/runtime/runtime-terminal-inspection'
import type { getSettingsForAgentTabRuntimeOwner } from '@/lib/agent-paste-draft'
import { AGENT_TUI_CLEAR_INPUT_MAX } from '../../../../shared/agent-tui-input-clear'
import {
  buildNativeChatImagePasteBytes,
  buildNativeChatPasteBytes,
  NATIVE_CHAT_SUBMIT
} from './native-chat-send'
import { enqueueNativeChatPtySend } from './native-chat-pty-send-queue'
import {
  NATIVE_CHAT_CLEAR_CONFIRM_MS,
  NATIVE_CHAT_CLEAR_UNSUBMITTED_INPUT,
  NATIVE_CHAT_IMAGE_ATTACHMENT_SETTLE_MS,
  NATIVE_CHAT_SUBMIT_DELAY_MS,
  type NativeChatSendHandle,
  type NativeChatSendOptions
} from './native-chat-runtime-send'

type RuntimeSettings = ReturnType<typeof getSettingsForAgentTabRuntimeOwner>

function clearInput(
  settings: RuntimeSettings,
  ptyId: string,
  options?: NativeChatSendOptions
): void {
  sendRuntimePtyInput(settings, ptyId, options?.clearInput ?? NATIVE_CHAT_CLEAR_UNSUBMITTED_INPUT)
}

/** User-facing send path: every content write and final Enter is acknowledged. */
export function sendNativeChatMessageReliably(
  settings: RuntimeSettings,
  ptyId: string,
  text: string,
  imagePaths: readonly string[] = [],
  options?: NativeChatSendOptions
): NativeChatSendHandle {
  const trimmedText = text.trim()
  const clearDelay = options?.confirmCleared ? NATIVE_CHAT_CLEAR_CONFIRM_MS : 0
  const durationMs =
    (imagePaths.length > 0 && trimmedText.length > 0
      ? NATIVE_CHAT_IMAGE_ATTACHMENT_SETTLE_MS + NATIVE_CHAT_SUBMIT_DELAY_MS
      : NATIVE_CHAT_SUBMIT_DELAY_MS) + clearDelay
  let resolveDelivery!: (delivered: boolean) => void
  let deliverySettled = false
  const delivery = new Promise<boolean>((resolve) => {
    resolveDelivery = resolve
  })
  const finishDelivery = (delivered: boolean): void => {
    if (!deliverySettled) {
      deliverySettled = true
      resolveDelivery(delivered)
    }
  }

  const queueHandle = enqueueNativeChatPtySend(
    ptyId,
    durationMs,
    ({ isCancelled, delay, markSubmitted }) => {
      const reject = (): void => {
        if (!isCancelled()) {
          clearInput(settings, ptyId, options)
        }
        finishDelivery(false)
        markSubmitted()
      }
      const verify = async (bytes: string): Promise<boolean> => {
        if (isCancelled()) {
          return false
        }
        try {
          return await sendRuntimePtyInputVerified(settings, ptyId, bytes)
        } catch {
          return false
        }
      }
      const submit = (): void => {
        delay(NATIVE_CHAT_SUBMIT_DELAY_MS, () => {
          void verify(NATIVE_CHAT_SUBMIT).then((accepted) => {
            if (!accepted) {
              return reject()
            }
            finishDelivery(true)
            markSubmitted()
          })
        })
      }
      const writeTextThenSubmit = (): void => {
        if (trimmedText.length === 0) {
          return submit()
        }
        void verify(buildNativeChatPasteBytes(text)).then((accepted) => {
          if (!accepted) {
            return reject()
          }
          submit()
        })
      }
      const writeContent = (): void => {
        void (async () => {
          for (const imagePath of imagePaths) {
            if (!(await verify(buildNativeChatImagePasteBytes(imagePath)))) {
              return reject()
            }
          }
          if (imagePaths.length > 0 && trimmedText.length > 0) {
            delay(NATIVE_CHAT_IMAGE_ATTACHMENT_SETTLE_MS, writeTextThenSubmit)
          } else {
            writeTextThenSubmit()
          }
        })()
      }

      if (isCancelled()) {
        return finishDelivery(false)
      }
      clearInput(settings, ptyId, options)
      if (!options?.confirmCleared) {
        return writeContent()
      }
      delay(NATIVE_CHAT_CLEAR_CONFIRM_MS, () => {
        let cleared = false
        try {
          cleared = options.confirmCleared?.() === true
        } catch {
          // Unreadable input is unconfirmed; widen the clear below.
        }
        if (!cleared) {
          sendRuntimePtyInput(settings, ptyId, AGENT_TUI_CLEAR_INPUT_MAX)
        }
        writeContent()
      })
    },
    { onCancelUnsubmitted: () => clearInput(settings, ptyId, options) }
  )

  return {
    ...queueHandle,
    delivery,
    cancel: () => {
      finishDelivery(false)
      queueHandle.cancel()
    }
  }
}
