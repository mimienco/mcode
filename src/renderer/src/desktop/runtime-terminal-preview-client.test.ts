import { describe, expect, it, vi } from 'vitest'
import { createRuntimeTerminalPreviewClient } from './runtime-terminal-preview-client'
import type { TauriTerminalPreviewClient } from './tauri-terminal-preview-client'

function createClient() {
  const detach = vi.fn()
  return {
    connect: vi.fn<TauriTerminalPreviewClient['connect']>().mockResolvedValue({
      snapshot: { data: '', cols: 80, rows: 24, seq: 1 }, replay: []
    }),
    input: vi.fn<TauriTerminalPreviewClient['input']>().mockResolvedValue(false),
    fit: vi.fn<TauriTerminalPreviewClient['fit']>().mockResolvedValue(null),
    ack: vi.fn<TauriTerminalPreviewClient['ack']>().mockResolvedValue(undefined),
    unsubscribe: vi.fn<TauriTerminalPreviewClient['unsubscribe']>().mockResolvedValue(undefined),
    onData: vi.fn<TauriTerminalPreviewClient['onData']>().mockReturnValue(detach)
  }
}

describe('runtime terminal preview routing', () => {
  for (const tauriRuntime of [false, true]) {
    it(`keeps every preview operation on ${tauriRuntime ? 'Tauri' : 'Electron'}`, async () => {
      const electron = createClient()
      const tauri = createClient()
      const electronProvider = vi.fn(() => electron)
      const client = createRuntimeTerminalPreviewClient({
        isTauri: () => tauriRuntime, electron: electronProvider, tauri
      })
      const selected = tauriRuntime ? tauri : electron
      const other = tauriRuntime ? electron : tauri
      const callback = vi.fn()
      expect(client.onData(callback)).toBe(selected.onData.mock.results[0]?.value)
      await expect(client.connect('pty-1', { scrollbackRows: 120 })).resolves.toMatchObject({ snapshot: { data: '' } })
      await expect(client.input('pty-1', 'paste')).resolves.toBe(false)
      await expect(client.fit('pty-1', 100, 30)).resolves.toBeNull()
      await client.ack('pty-1', 42)
      await client.unsubscribe('pty-1')
      expect(selected.connect).toHaveBeenCalledWith('pty-1', { scrollbackRows: 120 })
      expect(selected.input).toHaveBeenCalledWith('pty-1', 'paste')
      expect(selected.fit).toHaveBeenCalledWith('pty-1', 100, 30)
      expect(selected.ack).toHaveBeenCalledWith('pty-1', 42)
      expect(selected.unsubscribe).toHaveBeenCalledWith('pty-1')
      expect(selected.onData).toHaveBeenCalledWith(callback)
      for (const operation of Object.values(other)) {
        expect(operation).not.toHaveBeenCalled()
      }
      if (tauriRuntime) {
        expect(electronProvider).not.toHaveBeenCalled()
      }
    })
  }
})
