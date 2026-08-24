import { useCallback, type Dispatch, type SetStateAction } from 'react'
import { useAppStore } from '../../store'
import type { AgentType } from '../../../../shared/agent-status-types'
import type { NativeChatLaunchDraft } from '@/lib/native-chat-launch-prompt'
import { emitNativeChatMessageSent } from '@/lib/native-chat-telemetry'
import { isSlashCommandDraft } from '../../../../shared/native-chat-slash-commands'
import {
  sendNativeChatMessage,
  sendNativeChatTypedCommand,
  submitNativeChatPrompt,
  type NativeChatSendHandle
} from './native-chat-runtime-send'
import { sendNativeChatMessageReliably } from './native-chat-reliable-send'
import { resolveNativeChatLaunchDraftSend } from './native-chat-launch-draft-send'
import {
  buildNativeChatPrompt,
  nativeChatComposerTargetIsRemote
} from './native-chat-composer-target'
import {
  pushHistory,
  type HistoryState,
  type NativeChatSendClassification
} from './native-chat-composer-state'
import type { NativeChatComposerImageAttachment } from './NativeChatComposerField'
import type { NativeChatComposerFileReference } from './use-native-chat-composer-attachments'
import type { NativeChatDeliveryRecovery } from './use-native-chat-delivery-recovery'
import type { NativeChatSendLifecycle } from './use-native-chat-send-lifecycle'
import type { NativeChatPtySessionOptionsSurface } from './native-chat-pty-session-options'
import type { NativeChatResolvedTarget } from './native-chat-composer-target'

export function useNativeChatComposerSend(args: {
  agent: AgentType
  terminalTabId: string
  disabled: boolean
  isDispatchingSessionOption: boolean
  draft: string
  imageAttachments: readonly NativeChatComposerImageAttachment[]
  fileReferences: readonly NativeChatComposerFileReference[]
  classifySend: (draft: string) => NativeChatSendClassification
  resolveTarget: () => NativeChatResolvedTarget | null
  launchDraft: NativeChatLaunchDraft | null | undefined
  launchDraftResolved: boolean
  readTerminalScreen?: () => string | null
  onOptimisticSend?: (text: string, imagePaths?: string[]) => string | undefined
  onSlashCommand?: (command: string) => void
  sessionOptionsSurface: NativeChatPtySessionOptionsSurface | null
  trackPendingSend: NativeChatSendLifecycle['trackPendingSend']
  watchDelivery: NativeChatDeliveryRecovery
  setHistory: Dispatch<SetStateAction<HistoryState>>
  setDraft: (value: string) => void
  setCaret: Dispatch<SetStateAction<number>>
  clearSkillOrigin: () => void
  clearImageAttachments: () => void
  clearFileReferences: () => void
  setNotice: Dispatch<SetStateAction<string | null>>
}): () => void {
  const {
    agent,
    terminalTabId,
    disabled,
    isDispatchingSessionOption,
    draft,
    imageAttachments,
    fileReferences,
    classifySend,
    resolveTarget,
    launchDraft,
    launchDraftResolved,
    readTerminalScreen,
    onOptimisticSend,
    onSlashCommand,
    sessionOptionsSurface,
    trackPendingSend,
    watchDelivery,
    setHistory,
    setDraft,
    setCaret,
    clearSkillOrigin,
    clearImageAttachments,
    clearFileReferences,
    setNotice
  } = args

  return useCallback(() => {
    const filePaths = fileReferences.map((reference) => reference.path)
    const text = buildNativeChatPrompt(draft, filePaths)
    const imagePaths = imageAttachments.map((attachment) => attachment.path)
    if ((text.trim() === '' && imagePaths.length === 0) || disabled) {
      return
    }
    if (isDispatchingSessionOption) {
      return
    }
    const target = resolveTarget()
    if (!target) {
      return
    }
    // Why: selected file context turns slash-like text into an ordinary prompt.
    const classification = filePaths.length > 0 ? 'chat' : classifySend(draft)
    const { sendOptions } = resolveNativeChatLaunchDraftSend({
      launchDraft,
      launchDraftResolved,
      agent,
      readScreen: () => readTerminalScreen?.()
    })
    let pendingHandle: NativeChatSendHandle | null = null
    if (classification !== 'chat' && imagePaths.length === 0) {
      pendingHandle =
        agent === 'codex' && isSlashCommandDraft(text)
          ? sendNativeChatTypedCommand(target.settings, target.ptyId, text)
          : sendNativeChatMessage(target.settings, target.ptyId, text, sendOptions)
    } else if (imagePaths.length > 0 || text.trim().length > 0) {
      pendingHandle = sendNativeChatMessageReliably(
        target.settings,
        target.ptyId,
        text,
        imagePaths,
        sendOptions
      )
    } else {
      submitNativeChatPrompt(target.settings, target.ptyId)
    }
    if (classification !== 'chat') {
      if (pendingHandle) {
        trackPendingSend(pendingHandle)
      }
      if (classification === 'command') {
        onSlashCommand?.(text.trim())
        sessionOptionsSurface?.recordOutgoingCommand(text.trim())
      }
    } else {
      const pendingId = onOptimisticSend?.(text, imagePaths)
      if (pendingHandle) {
        trackPendingSend(pendingHandle, pendingId)
        watchDelivery(pendingHandle.delivery, pendingId, draft, imagePaths, filePaths)
      }
    }
    emitNativeChatMessageSent({
      agent,
      runtime: nativeChatComposerTargetIsRemote(target.ptyId) ? 'remote' : 'local'
    })
    setHistory((previous) => pushHistory(previous, text))
    setDraft('')
    setCaret(0)
    clearSkillOrigin()
    clearImageAttachments()
    clearFileReferences()
    setNotice(null)
    useAppStore.getState().clearNativeChatLaunchDraft(terminalTabId)
  }, [
    agent,
    classifySend,
    clearFileReferences,
    clearImageAttachments,
    clearSkillOrigin,
    disabled,
    draft,
    fileReferences,
    imageAttachments,
    isDispatchingSessionOption,
    launchDraft,
    launchDraftResolved,
    onOptimisticSend,
    onSlashCommand,
    readTerminalScreen,
    resolveTarget,
    sessionOptionsSurface,
    setCaret,
    setDraft,
    setHistory,
    setNotice,
    terminalTabId,
    trackPendingSend,
    watchDelivery
  ])
}
