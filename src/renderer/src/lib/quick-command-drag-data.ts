import {
  MAX_QUICK_COMMAND_AGENT_PROMPT_LENGTH,
  MAX_QUICK_COMMAND_LABEL_LENGTH
} from '../../../shared/terminal-quick-commands'

export const QUICK_COMMAND_DRAG_MIME = 'application/x-mcode-quick-command+json'
const QUICK_COMMAND_DRAG_FINISH_EVENT = 'mcode:quick-command-drag-finish'
let internalDropHandled = false
let activeInternalDrag: Omit<QuickCommandDragPayload, 'version'> | null = null

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
  activeInternalDrag = { label: payload.label, text: payload.text }
  dataTransfer.effectAllowed = 'copy'
  dataTransfer.setData(QUICK_COMMAND_DRAG_MIME, JSON.stringify(payload))
  dataTransfer.setData('text/plain', text)
  return true
}

export function markQuickCommandDropHandled(): void {
  internalDropHandled = true
}

/**
 * Gives eligible app-owned destinations one last synchronous chance to accept
 * a drag when Electron/xterm reports dragend without delivering a DOM drop.
 * The destination, rather than the source row, decides whether it is enabled
 * and whether the pointer is actually inside its surface.
 */
export function finishQuickCommandDrag(clientX: number, clientY: number): boolean {
  if (!internalDropHandled && activeInternalDrag) {
    window.dispatchEvent(
      new CustomEvent(QUICK_COMMAND_DRAG_FINISH_EVENT, {
        detail: { clientX, clientY }
      })
    )
  }
  const handled = internalDropHandled
  internalDropHandled = false
  activeInternalDrag = null
  return handled
}

export function subscribeQuickCommandDragFinish(
  onFinish: (
    payload: Omit<QuickCommandDragPayload, 'version'>,
    point: { clientX: number; clientY: number }
  ) => boolean
): () => void {
  const listener = (event: Event): void => {
    if (!(event instanceof CustomEvent) || !activeInternalDrag || internalDropHandled) {
      return
    }
    const point = readDragFinishPoint(event.detail)
    if (point && onFinish(activeInternalDrag, point)) {
      markQuickCommandDropHandled()
    }
  }
  window.addEventListener(QUICK_COMMAND_DRAG_FINISH_EVENT, listener)
  return () => window.removeEventListener(QUICK_COMMAND_DRAG_FINISH_EVENT, listener)
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

function readDragFinishPoint(value: unknown): { clientX: number; clientY: number } | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null
  }
  const point = value as Record<string, unknown>
  return typeof point.clientX === 'number' && typeof point.clientY === 'number'
    ? { clientX: point.clientX, clientY: point.clientY }
    : null
}
