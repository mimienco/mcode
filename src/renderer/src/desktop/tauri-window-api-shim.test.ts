// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const isTauriMock = vi.fn<(runtime?: unknown) => boolean>(() => false)

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: (runtime?: unknown) => isTauriMock(runtime),
  invoke: vi.fn(async () => undefined)
}))

type UiBridgeProbe = {
  readClipboardText: (options?: { maxBytes?: number }) => Promise<string>
  writeClipboardText: (text: string) => Promise<void>
  writeTerminalClipboardText: (text: string) => Promise<void>
  saveClipboardImageAsTempFile: (args?: { connectionId?: string | null }) => Promise<string | null>
  performNativePaste: (options?: { mode?: 'paste' | 'paste-and-match-style' }) => void
}

import type { installTauriWindowApiShim } from './tauri-window-api-shim'

function loadShim(): Promise<{ installTauriWindowApiShim: typeof installTauriWindowApiShim }> {
  return import('./tauri-window-api-shim')
}

describe('tauri-window-api-shim', () => {
  const originalApi = window.api

  beforeEach(async () => {
    window.api = undefined as unknown as Window['api']
    isTauriMock.mockReturnValue(false)
    const coreModule = await import('@tauri-apps/api/core')
    const invoke = coreModule.invoke as ReturnType<typeof vi.fn>
    invoke.mockReset()
  })

  it('does nothing under Electron when window.api already exists', async () => {
    const electronApi = {
      ui: {
        readClipboardText: () => Promise.resolve('electron')
      }
    } as unknown as Window['api']
    window.api = electronApi

    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    expect(window.api).toBe(electronApi)
  })

  it('does nothing under Electron when window.api is missing', async () => {
    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    expect(window.api).toBeUndefined()
  })

  it('installs a clipboard ui bridge over a missing window.api under Tauri', async () => {
    isTauriMock.mockReturnValue(true)
    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    const ui = (window.api as unknown as { ui: UiBridgeProbe }).ui
    expect(ui.readClipboardText).toBeTypeOf('function')
    expect(ui.writeClipboardText).toBeTypeOf('function')
    expect(ui.writeTerminalClipboardText).toBeTypeOf('function')
    expect(ui.saveClipboardImageAsTempFile).toBeTypeOf('function')
  })

  it('overrides only the clipboard members of an existing ui bridge', async () => {
    isTauriMock.mockReturnValue(true)
    const performNativePaste = vi.fn()
    window.api = {
      ui: {
        performNativePaste,
        readClipboardText: () => Promise.resolve('electron')
      }
    } as unknown as Window['api']

    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    const ui = (window.api as unknown as { ui: UiBridgeProbe }).ui
    expect(ui.performNativePaste).toBe(performNativePaste)
    await expect(ui.readClipboardText()).resolves.not.toBe('electron')
  })

  it('bridges writeClipboardText and readClipboardText through the core invoke', async () => {
    isTauriMock.mockReturnValue(true)
    const coreModule = await import('@tauri-apps/api/core')
    const invoke = coreModule.invoke as ReturnType<typeof vi.fn>
    invoke.mockResolvedValue('tauri text')

    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    const ui = (window.api as unknown as { ui: UiBridgeProbe }).ui
    await ui.writeClipboardText('payload')
    expect(invoke).toHaveBeenCalledWith('ui_clipboardWriteText', { text: 'payload' })

    await expect(ui.readClipboardText()).resolves.toBe('tauri text')
    expect(invoke).toHaveBeenCalledWith('ui_clipboardReadText', {
      maxBytes: 16 * 1024 * 1024
    })
  })

  it('saveClipboardImageAsTempFile returns null when ui_clipboardReadImage is unavailable', async () => {
    isTauriMock.mockReturnValue(true)
    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    const ui = (window.api as unknown as { ui: UiBridgeProbe }).ui
    await expect(ui.saveClipboardImageAsTempFile()).resolves.toBeNull()
  })

  it('saveClipboardImageAsTempFile returns the temp path from ui_clipboardReadImage', async () => {
    isTauriMock.mockReturnValue(true)
    const coreModule = await import('@tauri-apps/api/core')
    const invoke = coreModule.invoke as ReturnType<typeof vi.fn>
    invoke.mockResolvedValue('C:\\Temp\\mcode-paste-1.png')

    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    const ui = (window.api as unknown as { ui: UiBridgeProbe }).ui
    await expect(ui.saveClipboardImageAsTempFile()).resolves.toBe('C:\\Temp\\mcode-paste-1.png')
    expect(invoke).toHaveBeenCalledWith('ui_clipboardReadImage')
  })

  it('saveClipboardImageAsTempFile returns null for a remote connection without invoking', async () => {
    isTauriMock.mockReturnValue(true)
    const { installTauriWindowApiShim } = await loadShim()
    installTauriWindowApiShim()

    const ui = (window.api as unknown as { ui: UiBridgeProbe }).ui
    await expect(
      ui.saveClipboardImageAsTempFile({ connectionId: 'ssh-remote-1' })
    ).resolves.toBeNull()
  })

  afterEach(() => {
    window.api = originalApi
    vi.doUnmock('./tauri-clipboard-client')
  })
})
