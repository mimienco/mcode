import {
  MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH,
  MAX_QUICK_COMMAND_LABEL_LENGTH
} from '../../../shared/terminal-quick-commands'

export const QUICK_COMMAND_DRAG_MIME = 'application/x-mcode-quick-command+json'
let internalDropHandled = false

type QuickCommandDragPayload = {
  version: 1
  label: string
  text: string
}

export function hasQuickCommandDragData(dataTransfer: DataTransfer): boolean {
  return Array.from(dataTransfer.types).includes(QUICK_COMMAND_DRAG_MIME)
}

export function writeQuickCommandDragData(
  dataTransfer: DataTransfer,
  label: string,
  text: string
): boolean {
  if (!isValidQuickCommandDragText(text)) {
    return false
  }
  internalDropHandled = false
  const payload: QuickCommandDragPayload = {
    version: 1,
    label: label.slice(0, MAX_QUICK_COMMAND_LABEL_LENGTH),
    text
  }
  dataTransfer.effectAllowed = 'copy'
  dataTransfer.setData(QUICK_COMMAND_DRAG_MIME, JSON.stringify(payload))
  dataTransfer.setData('text/plain', text)
  return true
}

export function markQuickCommandDropHandled(): void {
  internalDropHandled = true
}

export function consumeQuickCommandDropHandled(): boolean {
  const handled = internalDropHandled
  internalDropHandled = false
  return handled
}

export function readQuickCommandDragData(
  dataTransfer: DataTransfer
): Omit<QuickCommandDragPayload, 'version'> | null {
  if (!hasQuickCommandDragData(dataTransfer)) {
    return null
  }
  try {
    const payload = JSON.parse(dataTransfer.getData(QUICK_COMMAND_DRAG_MIME)) as unknown
    if (!isQuickCommandDragPayload(payload)) {
      return null
    }
    return { label: payload.label, text: payload.text }
  } catch {
    return null
  }
}

function isQuickCommandDragPayload(value: unknown): value is QuickCommandDragPayload {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return false
  }
  const payload = value as Record<string, unknown>
  return (
    Object.keys(payload).length === 3 &&
    payload.version === 1 &&
    typeof payload.label === 'string' &&
    payload.label.length <= MAX_QUICK_COMMAND_LABEL_LENGTH &&
    typeof payload.text === 'string' &&
    isValidQuickCommandDragText(payload.text)
  )
}

function isValidQuickCommandDragText(text: string): boolean {
  return (
    text.trim().length > 0 &&
    text.length <= MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH &&
    !text.includes('\0')
  )
}
