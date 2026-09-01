import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  CLIPBOARD_TEXT_READ_MAX_BYTES,
  createTauriClipboardClient,
  type ClipboardInvoke
} from './tauri-clipboard-client'

function createInvoke(
  overrides: {
    resolve?: (command: string, args?: Record<string, unknown>) => unknown
    reject?: (command: string, args?: Record<string, unknown>) => unknown
  } = {}
): ClipboardInvoke & { calls: { command: string; args?: Record<string, unknown> }[] } {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = (async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    if (overrides.reject) {
      throw overrides.reject(command, args)
    }
    return overrides.resolve?.(command, args)
  }) as ClipboardInvoke & { calls: typeof calls }
  invoke.calls = calls
  return invoke
}

describe('tauri-clipboard-client', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('routes writes and terminal writes to the renamed Rust commands', async () => {
    const invoke = createInvoke()
    const client = createTauriClipboardClient(invoke)

    await client.writeClipboardText('hello')
    await client.writeTerminalClipboardText('terminal')

    expect(invoke.calls).toEqual([
      { command: 'ui_clipboardWriteText', args: { text: 'hello' } },
      { command: 'ui_clipboardWriteTerminalText', args: { text: 'terminal' } }
    ])
  })

  it('reads text and forwards the resolved byte cap', async () => {
    const invoke = createInvoke({ resolve: () => 'clipboard payload' })
    const client = createTauriClipboardClient(invoke)

    await expect(client.readClipboardText()).resolves.toBe('clipboard payload')
    await expect(client.readClipboardText({ maxBytes: 2048 })).resolves.toBe('clipboard payload')

    expect(invoke.calls).toEqual([
      {
        command: 'ui_clipboardReadText',
        args: { maxBytes: CLIPBOARD_TEXT_READ_MAX_BYTES }
      },
      { command: 'ui_clipboardReadText', args: { maxBytes: 2048 } }
    ])
  })

  it('normalizes Rust too-large read rejections into the renderer error contract', async () => {
    const invoke = createInvoke({
      reject: () => 'clipboard text exceeds 16777216 bytes'
    })
    const client = createTauriClipboardClient(invoke)

    await expect(client.readClipboardText()).rejects.toThrow(
      'Clipboard text is too large for this paste target.'
    )
  })

  it('passes through non-string invoke rejections unchanged', async () => {
    const failure = new Error('clipboard unavailable')
    const invoke = createInvoke({ reject: () => failure })
    const client = createTauriClipboardClient(invoke)

    await expect(client.writeClipboardText('x')).rejects.toBe(failure)
  })

  it('rejects oversized writes before touching the backend', async () => {
    const invoke = createInvoke()
    const client = createTauriClipboardClient(invoke)

    await expect(
      client.writeClipboardText('x'.repeat(CLIPBOARD_TEXT_READ_MAX_BYTES + 1))
    ).rejects.toThrow('Clipboard text is too large to copy safely.')
    expect(invoke.calls).toEqual([])
  })
})
