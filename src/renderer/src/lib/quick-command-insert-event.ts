import { MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH } from '../../../shared/terminal-quick-commands'

const QUICK_COMMAND_INSERT_EVENT = 'mcode:quick-command-insert'
const QUICK_COMMAND_CHAT_INSERT_EVENT = 'mcode:quick-command-chat-insert'

type QuickCommandInsertDetail = {
  targetTabId: string
  targetPaneId?: number
  text: string
}

type QuickCommandChatInsertDetail = {
  targetTabId: string
  targetLeafId: string
  text: string
}

export function dispatchQuickCommandInsert(
  targetTabId: string,
  text: string,
  targetPaneId?: number
): void {
  window.dispatchEvent(
    new CustomEvent<QuickCommandInsertDetail>(QUICK_COMMAND_INSERT_EVENT, {
      detail: { targetTabId, text, ...(targetPaneId === undefined ? {} : { targetPaneId }) }
    })
  )
}

export function subscribeQuickCommandInsert(
  targetTabId: string,
  onInsert: (text: string, targetPaneId?: number) => void
): () => void {
  const listener = (event: Event): void => {
    const detail = (event as CustomEvent<QuickCommandInsertDetail>).detail
    if (
      detail?.targetTabId === targetTabId &&
      isValidQuickCommandInsertText(detail.text) &&
      (detail.targetPaneId === undefined ||
        (Number.isInteger(detail.targetPaneId) && detail.targetPaneId >= 0))
    ) {
      onInsert(detail.text, detail.targetPaneId)
    }
  }
  window.addEventListener(QUICK_COMMAND_INSERT_EVENT, listener)
  return () => window.removeEventListener(QUICK_COMMAND_INSERT_EVENT, listener)
}

export function dispatchQuickCommandChatInsert(
  targetTabId: string,
  targetLeafId: string,
  text: string
): void {
  window.dispatchEvent(
    new CustomEvent<QuickCommandChatInsertDetail>(QUICK_COMMAND_CHAT_INSERT_EVENT, {
      detail: { targetTabId, targetLeafId, text }
    })
  )
}

export function subscribeQuickCommandChatInsert(
  targetTabId: string,
  targetLeafId: string,
  onInsert: (text: string) => void
): () => void {
  const listener = (event: Event): void => {
    const detail = (event as CustomEvent<QuickCommandChatInsertDetail>).detail
    if (
      detail?.targetTabId === targetTabId &&
      detail.targetLeafId === targetLeafId &&
      isValidQuickCommandInsertText(detail.text)
    ) {
      onInsert(detail.text)
    }
  }
  window.addEventListener(QUICK_COMMAND_CHAT_INSERT_EVENT, listener)
  return () => window.removeEventListener(QUICK_COMMAND_CHAT_INSERT_EVENT, listener)
}

function isValidQuickCommandInsertText(text: unknown): text is string {
  return (
    typeof text === 'string' &&
    text.trim().length > 0 &&
    text.length <= MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH &&
    !text.includes('\0')
  )
}
