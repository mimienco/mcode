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

type SpeechBridgeProbe = {
  getCatalog: () => Promise<unknown[]>
  getModelStates: () => Promise<unknown[]>
  startDictation: (
    modelId: string,
    hotwords: string[] | undefined,
    sessionId: string
  ) => Promise<void>
  feedAudio: (samples: Float32Array, sampleRate: number, sessionId?: string) => Promise<void>
  stopDictation: (sessionId?: string) => Promise<void>
  onPartialTranscript: (callback: (data: unknown) => void) => () => void
}

type TerminalPreviewBridgeProbe = {
  connect: (ptyId: string, opts?: { scrollbackRows?: number }) => Promise<unknown>
  input: (ptyId: string, data: string) => Promise<boolean>
  fit: (ptyId: string, cols: number, rows: number) => Promise<{ cols: number; rows: number } | null>
  ack: (ptyId: string, bytes: number) => Promise<void>
  unsubscribe: (ptyId: string) => Promise<void>
  onData: (callback: (payload: unknown) => void) => () => void
}

type WindowApiProbe = {
  ui: UiBridgeProbe
  speech: SpeechBridgeProbe
  terminalPreview: TerminalPreviewBridgeProbe
}

import type { installTauriWindowApiShim } from './tauri-window-api-shim'

function loadShim(): Promise<{ installTauriWindowApiShim: typeof installTauriWindowApiShim }> {
  return import('./tauri-window-api-shim')
}

function apiProbe(): WindowApiProbe {
  return window.api as unknown as WindowApiProbe
}

async function installUnderTauri(invokeResult?: unknown): Promise<ReturnType<typeof vi.fn>> {
  isTauriMock.mockReturnValue(true)
  const coreModule = await import('@tauri-apps/api/core')
  const invoke = coreModule.invoke as ReturnType<typeof vi.fn>
  if (invokeResult !== undefined) {
    invoke.mockResolvedValue(invokeResult)
  }
  const { installTauriWindowApiShim } = await loadShim()
  installTauriWindowApiShim()
  return invoke
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
    await installUnderTauri()

    const ui = apiProbe().ui
    expect(ui.readClipboardText).toBeTypeOf('function')
    expect(ui.writeClipboardText).toBeTypeOf('function')
    expect(ui.writeTerminalClipboardText).toBeTypeOf('function')
    expect(ui.saveClipboardImageAsTempFile).toBeTypeOf('function')
  })

  it('installs a terminalPreview bridge over a missing window.api under Tauri', async () => {
    await installUnderTauri()

    const terminalPreview = apiProbe().terminalPreview
    expect(terminalPreview.connect).toBeTypeOf('function')
    expect(terminalPreview.input).toBeTypeOf('function')
    expect(terminalPreview.fit).toBeTypeOf('function')
    expect(terminalPreview.ack).toBeTypeOf('function')
    expect(terminalPreview.unsubscribe).toBeTypeOf('function')
    expect(terminalPreview.onData).toBeTypeOf('function')
  })

  it('routes terminalPreview calls through the renamed Rust commands', async () => {
    const invoke = await installUnderTauri({ seq: 3, chunk: null, snapshot: 'screen' })

    const terminalPreview = apiProbe().terminalPreview
    await expect(terminalPreview.connect('pty-1')).resolves.toEqual({
      snapshot: { data: 'screen', cols: 80, rows: 24, seq: 3 },
      replay: []
    })
    await expect(terminalPreview.input('pty-1', 'x')).resolves.toBe(true)
    expect(invoke).toHaveBeenCalledWith('terminalPreviewConnect', { ptyId: 'pty-1' })
    expect(invoke).toHaveBeenCalledWith('terminalPreviewInput', { ptyId: 'pty-1', data: 'x' })
  })

  it('installs a speech bridge over a missing window.api under Tauri', async () => {
    await installUnderTauri()

    const speech = apiProbe().speech
    expect(speech.getCatalog).toBeTypeOf('function')
    expect(speech.getModelStates).toBeTypeOf('function')
    expect(speech.startDictation).toBeTypeOf('function')
    expect(speech.feedAudio).toBeTypeOf('function')
    expect(speech.stopDictation).toBeTypeOf('function')
    expect(speech.onPartialTranscript).toBeTypeOf('function')
  })

  it('overrides only the clipboard members of an existing ui bridge', async () => {
    const performNativePaste = vi.fn()
    window.api = {
      ui: {
        performNativePaste,
        readClipboardText: () => Promise.resolve('electron')
      }
    } as unknown as Window['api']

    await installUnderTauri()

    const ui = apiProbe().ui
    expect(ui.performNativePaste).toBe(performNativePaste)
    await expect(ui.readClipboardText()).resolves.not.toBe('electron')
  })

  it('bridges writeClipboardText and readClipboardText through the core invoke', async () => {
    const invoke = await installUnderTauri('tauri text')

    const ui = apiProbe().ui
    await ui.writeClipboardText('payload')
    expect(invoke).toHaveBeenCalledWith('ui_clipboardWriteText', { text: 'payload' })

    await expect(ui.readClipboardText()).resolves.toBe('tauri text')
    expect(invoke).toHaveBeenCalledWith('ui_clipboardReadText', {
      maxBytes: 16 * 1024 * 1024
    })
  })

  it('routes speech dictation calls through the renamed Rust commands', async () => {
    const invoke = await installUnderTauri([])

    const speech = apiProbe().speech
    await speech.startDictation('model-1', undefined, 'session-1')
    expect(invoke).toHaveBeenCalledWith('ui_speechStartDictation', {
      sessionId: 'session-1',
      owner: 'desktop',
      modelId: 'model-1'
    })

    await speech.feedAudio(new Float32Array([0.5, 1]), 16000, 'session-1')
    expect(invoke).toHaveBeenCalledWith('ui_speechFeedAudio', {
      sessionId: 'session-1',
      samples: [0.5, 1],
      sampleRate: 16000
    })

    await speech.stopDictation('session-1')
    expect(invoke).toHaveBeenCalledWith('ui_speechStopDictation', { sessionId: 'session-1' })
  })

  it('maps the Rust model-state record into the Electron state array', async () => {
    await installUnderTauri({
      'whisper-tiny': 'ready',
      'zipformer-streaming-zh-14m': 'idle',
      'paraformer-bilingual-zh-en': 'downloading'
    })

    const speech = apiProbe().speech
    await expect(speech.getModelStates()).resolves.toEqual([
      { id: 'whisper-tiny', status: 'ready' },
      { id: 'zipformer-streaming-zh-14m', status: 'not-downloaded' },
      { id: 'paraformer-bilingual-zh-en', status: 'downloading' }
    ])
  })

  it('lets the Tauri bridge override an existing speech member', async () => {
    const electronGetCatalog = (): Promise<unknown[]> => Promise.resolve([{ id: 'electron-only' }])
    window.api = {
      speech: {
        getCatalog: electronGetCatalog
      }
    } as unknown as Window['api']

    await installUnderTauri([])

    const speech = apiProbe().speech
    expect(speech.getCatalog).not.toBe(electronGetCatalog)
    await expect(speech.getCatalog()).resolves.not.toEqual([{ id: 'electron-only' }])
    await expect(speech.startDictation('m', undefined, 's')).resolves.toBeUndefined()
  })

  it('returns a synchronous unlisten from speech subscriptions', async () => {
    await installUnderTauri()

    const speech = apiProbe().speech
    const unlisten = speech.onPartialTranscript(() => undefined)
    expect(typeof unlisten).toBe('function')
    expect(() => unlisten()).not.toThrow()
  })

  it('saveClipboardImageAsTempFile returns null when ui_clipboardReadImage is unavailable', async () => {
    await installUnderTauri()

    const ui = apiProbe().ui
    await expect(ui.saveClipboardImageAsTempFile()).resolves.toBeNull()
  })

  it('saveClipboardImageAsTempFile returns the temp path from ui_clipboardReadImage', async () => {
    const invoke = await installUnderTauri('C:\\Temp\\mcode-paste-1.png')

    const ui = apiProbe().ui
    await expect(ui.saveClipboardImageAsTempFile()).resolves.toBe('C:\\Temp\\mcode-paste-1.png')
    expect(invoke).toHaveBeenCalledWith('ui_clipboardReadImage')
  })

  it('saveClipboardImageAsTempFile returns null for a remote connection without invoking', async () => {
    await installUnderTauri()

    const ui = apiProbe().ui
    await expect(
      ui.saveClipboardImageAsTempFile({ connectionId: 'ssh-remote-1' })
    ).resolves.toBeNull()
  })

  afterEach(() => {
    window.api = originalApi
    vi.doUnmock('./tauri-clipboard-client')
  })
})
