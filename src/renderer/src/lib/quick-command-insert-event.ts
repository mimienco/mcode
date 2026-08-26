import { MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH } from '../../../shared/terminal-quick-commands'

const QUICK_COMMAND_INSERT_EVENT = 'mcode:quick-command-insert'

type QuickCommandInsertDetail = {
  targetTabId: string
  targetPaneId?: number
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
      typeof detail.text === 'string' &&
      detail.text.trim().length > 0 &&
      detail.text.length <= MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH &&
      !detail.text.includes('\0') &&
      (detail.targetPaneId === undefined ||
        (Number.isInteger(detail.targetPaneId) && detail.targetPaneId >= 0))
    ) {
      onInsert(detail.text, detail.targetPaneId)
    }
  }
  window.addEventListener(QUICK_COMMAND_INSERT_EVENT, listener)
  return () => window.removeEventListener(QUICK_COMMAND_INSERT_EVENT, listener)
}
