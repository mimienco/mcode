import { invoke } from '@tauri-apps/api/core'

/**
 * Capability shape shared with the Electron `window.api.ui` clipboard bridge,
 * narrowed to the methods the Tauri runtime serves natively.
 */
export type TauriClipboardClient = {
  readClipboardText: (options?: ReadClipboardTextOptions) => Promise<string>
  writeClipboardText: (text: string) => Promise<void>
  writeTerminalClipboardText: (text: string) => Promise<void>
}

export type ReadClipboardTextOptions = {
  maxBytes?: number
}

/**
 * Mirrors `CLIPBOARD_TEXT_READ_MAX_BYTES` from `src/shared/clipboard-text.ts`;
 * must stay in sync with the Rust caps in `src-tauri/src/clipboard_commands.rs`.
 */
export const CLIPBOARD_TEXT_READ_MAX_BYTES = 16 * 1024 * 1024
export const CLIPBOARD_TEXT_WRITE_MAX_BYTES = 16 * 1024 * 1024

const CLIPBOARD_TEXT_TOO_LARGE_ERROR = 'Clipboard text is too large for this paste target.'
const CLIPBOARD_TEXT_WRITE_TOO_LARGE_ERROR = 'Clipboard text is too large to copy safely.'

export type ClipboardInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

/**
 * Tauri invoke-backed clipboard capability. The Rust side enforces the byte
 * caps; this client keeps the renderer's too-large error contract intact by
 * surfacing the same message strings the Electron bridge throws.
 */
export function createTauriClipboardClient(invoke: ClipboardInvoke): TauriClipboardClient {
  return {
    async readClipboardText(options) {
      const maxBytes =
        Number.isFinite(options?.maxBytes) && (options?.maxBytes ?? 0) > 0
          ? Math.floor(options?.maxBytes ?? CLIPBOARD_TEXT_READ_MAX_BYTES)
          : CLIPBOARD_TEXT_READ_MAX_BYTES
      try {
        const text = await invoke('ui_clipboardReadText', { maxBytes })
        return typeof text === 'string' ? text : ''
      } catch (error) {
        throw normalizeClipboardError(error)
      }
    },
    async writeClipboardText(text) {
      if (text.length > CLIPBOARD_TEXT_WRITE_MAX_BYTES) {
        throw new Error(CLIPBOARD_TEXT_WRITE_TOO_LARGE_ERROR)
      }
      try {
        await invoke('ui_clipboardWriteText', { text })
      } catch (error) {
        throw normalizeClipboardError(error)
      }
    },
    async writeTerminalClipboardText(text) {
      if (text.length > CLIPBOARD_TEXT_WRITE_MAX_BYTES) {
        throw new Error(CLIPBOARD_TEXT_WRITE_TOO_LARGE_ERROR)
      }
      try {
        await invoke('ui_clipboardWriteTerminalText', { text })
      } catch (error) {
        throw normalizeClipboardError(error)
      }
    }
  }
}

function normalizeClipboardError(error: unknown): Error {
  if (error instanceof Error) {
    return error
  }
  const message = typeof error === 'string' ? error : JSON.stringify(error)
  if (message.includes('clipboard text exceeds')) {
    return new Error(CLIPBOARD_TEXT_TOO_LARGE_ERROR)
  }
  return new Error(message)
}

export const tauriClipboardClient = createTauriClipboardClient((command, args) =>
  invoke(command, args)
)
