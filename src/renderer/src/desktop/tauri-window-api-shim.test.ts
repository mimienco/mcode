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

type SessionBridgeProbe = {
  get: () => Promise<unknown>
  set: (state: unknown) => Promise<void>
  patch: (patch: unknown) => Promise<void>
  flush: () => Promise<void>
  open: (args: { folderPath: string }) => Promise<unknown>
  switch: (args: { activeWorktreeId: string }) => Promise<void>
  listRecent: () => Promise<unknown>
}

type AgentStatusBridgeProbe = {
  onSet: (callback: (data: unknown) => void) => () => void
  onClear: (callback: (data: unknown) => void) => () => void
  getSnapshot: () => Promise<unknown[]>
  inferInterrupt: (request: unknown) => Promise<boolean>
  drop: (paneKey: string) => void
}

type AgentHooksBridgeProbe = {
  claudeStatus: () => Promise<unknown>
  openClaudeStatus: () => Promise<unknown>
  kimiStatus: () => Promise<unknown>
}

type WindowApiProbe = {
  ui: UiBridgeProbe
  speech: SpeechBridgeProbe
  terminalPreview: TerminalPreviewBridgeProbe
  session: SessionBridgeProbe
  agentStatus: AgentStatusBridgeProbe
  agentHooks: AgentHooksBridgeProbe
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

  it('installs a session bridge over a missing window.api under Tauri', async () => {
    const invoke = await installUnderTauri()
    invoke.mockImplementation(async (command: string) => {
      if (command === 'workspace_open') {
        return {
          kind: 'repo',
          repo: { id: 'repo-1' },
          folderWorkspace: null,
          alreadyExisted: false
        }
      }
      if (command === 'workspace_list_recent') {
        return {
          repos: [],
          folderWorkspaces: [],
          lastVisitedAtByWorktreeId: {},
          activeWorkspaceKey: null,
          activeRepoId: null
        }
      }
      return undefined
    })
    const session = apiProbe().session as unknown as SessionBridgeProbe
    expect(session.open).toBeTypeOf('function')
    expect(session.switch).toBeTypeOf('function')
    expect(session.listRecent).toBeTypeOf('function')
    await expect(session.open({ folderPath: 'D:/repo' })).resolves.toMatchObject({
      kind: 'repo'
    })
    await expect(session.switch({ activeWorktreeId: 'wt-1' })).resolves.toBeUndefined()
    await expect(session.listRecent()).resolves.toMatchObject({ repos: [] })
    expect(invoke).toHaveBeenCalledWith('workspace_open', {
      args: { folderPath: 'D:/repo' }
    })
    expect(invoke).toHaveBeenCalledWith('workspace_switch', {
      args: { activeWorktreeId: 'wt-1' }
    })
  })

  it('installs agentStatus and agentHooks bridges over a missing window.api under Tauri', async () => {
    await installUnderTauri()

    const agentStatus = apiProbe().agentStatus
    const agentHooks = apiProbe().agentHooks
    expect(agentStatus.onSet).toBeTypeOf('function')
    expect(agentStatus.onClear).toBeTypeOf('function')
    expect(agentStatus.getSnapshot).toBeTypeOf('function')
    expect(agentStatus.inferInterrupt).toBeTypeOf('function')
    expect(agentStatus.drop).toBeTypeOf('function')
    expect(agentHooks.claudeStatus).toBeTypeOf('function')
    expect(agentHooks.openClaudeStatus).toBeTypeOf('function')
    expect(agentHooks.kimiStatus).toBeTypeOf('function')
  })

  it('routes agentStatus snapshot and agentHooks status through the Rust commands', async () => {
    const invoke = await installUnderTauri()
    invoke.mockImplementation(async (command: string) => {
      if (command === 'agent_status_get_snapshot') {
        return []
      }
      if (command === 'agent_hooks_status') {
        return {
          agent: 'claude',
          state: 'skipped',
          configPath: '',
          managedHooksPresent: false,
          detail: 'not yet ported to Tauri (wave 38)'
        }
      }
      return undefined
    })

    await expect(apiProbe().agentStatus.getSnapshot()).resolves.toEqual([])
    await expect(apiProbe().agentHooks.claudeStatus()).resolves.toMatchObject({
      agent: 'claude',
      state: 'skipped'
    })
    expect(invoke).toHaveBeenCalledWith('agent_status_get_snapshot', undefined)
    expect(invoke).toHaveBeenCalledWith('agent_hooks_status', { agent: 'claude' })
  })

  it('returns a synchronous unlisten from agentStatus subscriptions', async () => {
    await installUnderTauri()

    const unlisten = apiProbe().agentStatus.onSet(() => undefined)
    expect(typeof unlisten).toBe('function')
    expect(() => unlisten()).not.toThrow()
  })

  it('lets the Tauri agentStatus bridge override an existing presence stub', async () => {
    const electronGetSnapshot = (): Promise<unknown[]> => Promise.resolve([{ paneKey: 'electron' }])
    window.api = {
      agentStatus: {
        getSnapshot: electronGetSnapshot
      }
    } as unknown as Window['api']

    const invoke = await installUnderTauri([])
    const agentStatus = apiProbe().agentStatus
    expect(agentStatus.getSnapshot).not.toBe(electronGetSnapshot)
    await expect(agentStatus.getSnapshot()).resolves.toEqual([])
    expect(invoke).toHaveBeenCalledWith('agent_status_get_snapshot', undefined)
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

  it('routes the actual preload session contract to durable native commands', async () => {
    const state = {
      activeRepoId: null,
      activeWorktreeId: null,
      activeTabId: null,
      tabsByWorktree: {},
      terminalLayoutsByTabId: {}
    }
    const invoke = await installUnderTauri(state)
    const session = apiProbe().session
    await expect(session.get()).resolves.toEqual(state)
    await session.set(state)
    await session.patch({ activeTabId: null })
    await session.flush()
    expect(invoke).toHaveBeenCalledWith('session_get', { hostId: null })
    expect(invoke).toHaveBeenCalledWith('session_set', { args: state, hostId: null })
    expect(invoke).toHaveBeenCalledWith('session_patch', {
      args: { activeTabId: null },
      hostId: null
    })
    expect(invoke).toHaveBeenCalledWith('session_flush', undefined)
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
