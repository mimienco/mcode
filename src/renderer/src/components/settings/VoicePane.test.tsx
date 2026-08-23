// @vitest-environment happy-dom

import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DeveloperPermissionRequestResult } from '../../../../shared/developer-permissions-types'
import type { SpeechModelManifest } from '../../../../shared/speech-types'
import type { GlobalSettings } from '../../../../shared/global-settings-types'
import { getDefaultVoiceSettings } from '../../../../shared/constants'
import { handleVoiceDictationToggle, VoicePane } from './VoicePane'
import { TooltipProvider } from '../ui/tooltip'

const { useAppStoreMock, useShortcutLabelMock } = vi.hoisted(() => ({
  useAppStoreMock: vi.fn(),
  useShortcutLabelMock: vi.fn()
}))

vi.mock('@/store', () => ({ useAppStore: useAppStoreMock }))

vi.mock('@/hooks/useShortcutLabel', () => ({
  useShortcutLabel: useShortcutLabelMock,
  useOptionalShortcutLabel: useShortcutLabelMock
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    message: vi.fn(),
    success: vi.fn()
  }
}))

const deniedMicrophoneResult: DeveloperPermissionRequestResult = {
  id: 'microphone',
  status: 'denied',
  openedSystemSettings: false
}
const EMPTY_SPEECH_CATALOG: SpeechModelManifest[] = []

function makeSettings(voiceEnabled?: boolean): GlobalSettings {
  if (voiceEnabled === undefined) {
    return {} as GlobalSettings
  }
  return {
    voice: {
      ...getDefaultVoiceSettings(),
      enabled: voiceEnabled
    }
  } as GlobalSettings
}

function installWindowApi(
  requestMicrophonePermission: () => Promise<DeveloperPermissionRequestResult>
) {
  Object.assign(window, {
    api: {
      developerPermissions: {
        request: vi.fn(requestMicrophonePermission)
      },
      speech: {
        getCatalog: vi.fn(async () => EMPTY_SPEECH_CATALOG),
        getOpenAiApiKeyStatus: vi.fn(async () => ({ configured: false })),
        saveOpenAiApiKey: vi.fn(async () => ({ configured: true })),
        clearOpenAiApiKey: vi.fn(async () => ({ configured: false })),
        onDownloadProgress: vi.fn(() => () => {}),
        downloadModel: vi.fn()
      },
      ui: {
        setShortcutRecorderFocused: vi.fn()
      }
    }
  })
}

async function renderVoicePane(args: {
  voiceEnabled?: boolean
  markFeatureTipsSeen: (ids: string[]) => void
  updateSettings: (updates: Partial<GlobalSettings>) => void
  requestMicrophonePermission?: () => Promise<DeveloperPermissionRequestResult>
  recordFeatureInteraction?: (id: string) => void
}): Promise<{
  button: HTMLButtonElement
  root: Root
  container: HTMLDivElement
  refreshModelStates: ReturnType<typeof vi.fn>
  setKeybindingOverride: ReturnType<typeof vi.fn>
}> {
  const refreshModelStates = vi.fn()
  const setKeybindingOverride = vi.fn(async () => undefined)
  useAppStoreMock.mockImplementation((selector: (state: Record<string, unknown>) => unknown) =>
    selector({
      modelStates: [],
      refreshModelStates,
      markFeatureTipsSeen: args.markFeatureTipsSeen,
      recordFeatureInteraction: args.recordFeatureInteraction ?? vi.fn(),
      keybindings: {},
      keybindingSnapshot: null,
      setKeybindingOverride,
      resetKeybindingOverride: vi.fn(async () => undefined)
    })
  )
  useShortcutLabelMock.mockReturnValue('Ctrl+Shift+Y')
  installWindowApi(args.requestMicrophonePermission ?? vi.fn(async () => deniedMicrophoneResult))

  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  await act(async () => {
    root.render(
      <TooltipProvider>
        <VoicePane
          settings={makeSettings(args.voiceEnabled)}
          updateSettings={args.updateSettings}
        />
      </TooltipProvider>
    )
  })

  const button = container.querySelector<HTMLButtonElement>('button[role="switch"]')
  if (!button) {
    throw new Error('Voice Dictation switch was not rendered')
  }

  return { button, root, container, refreshModelStates, setKeybindingOverride }
}

async function clickSwitch(button: HTMLButtonElement): Promise<void> {
  await act(async () => {
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  })
  await act(async () => {
    await Promise.resolve()
  })
}

describe('VoicePane', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    document.body.innerHTML = ''
  })

  beforeEach(() => {
    useAppStoreMock.mockReset()
    useShortcutLabelMock.mockReset()
  })

  it('fetches speech data once across re-renders when voice settings are absent', async () => {
    const updateSettings = vi.fn()
    const { root, refreshModelStates } = await renderVoicePane({
      markFeatureTipsSeen: vi.fn(),
      updateSettings
    })

    for (let i = 0; i < 4; i++) {
      await act(async () => {
        root.render(
          <TooltipProvider>
            <VoicePane settings={{} as GlobalSettings} updateSettings={updateSettings} />
          </TooltipProvider>
        )
      })
    }
    act(() => root.unmount())

    expect(window.api.speech.getCatalog).toHaveBeenCalledTimes(1)
    expect(refreshModelStates).toHaveBeenCalledTimes(1)
  })

  it('lets the user rebind dictation from the Voice pane', async () => {
    const { container, root, setKeybindingOverride } = await renderVoicePane({
      voiceEnabled: true,
      markFeatureTipsSeen: vi.fn(),
      updateSettings: vi.fn()
    })

    const shortcut = container.querySelector<HTMLButtonElement>(
      'button[aria-label="Change shortcut for Dictation"]'
    )
    expect(shortcut).not.toBeNull()

    await act(async () => {
      shortcut?.click()
    })
    await act(async () => {
      shortcut?.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'Y',
          code: 'KeyY',
          ctrlKey: true,
          shiftKey: true,
          bubbles: true
        })
      )
      await Promise.resolve()
    })

    expect(setKeybindingOverride).toHaveBeenCalledWith('voice.dictation', ['Mod+Shift+Y'])
    root.unmount()
  })

  it('keeps a conflicting dictation shortcut unsaved and explains the conflict', async () => {
    const { container, root, setKeybindingOverride } = await renderVoicePane({
      voiceEnabled: true,
      markFeatureTipsSeen: vi.fn(),
      updateSettings: vi.fn()
    })
    const shortcut = container.querySelector<HTMLButtonElement>(
      'button[aria-label="Change shortcut for Dictation"]'
    )

    await act(async () => {
      shortcut?.click()
    })
    await act(async () => {
      shortcut?.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'B',
          code: 'KeyB',
          ctrlKey: true,
          bubbles: true
        })
      )
      await Promise.resolve()
    })

    expect(setKeybindingOverride).not.toHaveBeenCalled()
    expect(container.textContent).toContain('conflicts with Toggle Sidebar')
    root.unmount()
  })

  it('clicking the switch marks the voice tip seen before disabling voice settings', async () => {
    const calls: string[] = []
    const requestMicrophonePermission = vi.fn()
    const updateVoiceSettings = vi.fn((updates: { enabled?: boolean }) => {
      calls.push(`settings:${String(updates.enabled)}`)
    })

    await handleVoiceDictationToggle({
      voiceEnabled: true,
      markFeatureTipsSeen: (ids) => calls.push(`seen:${ids.join(',')}`),
      updateVoiceSettings,
      requestMicrophonePermission
    })

    expect(calls).toEqual(['seen:voice-dictation', 'settings:false'])
    expect(updateVoiceSettings).toHaveBeenCalledWith({ enabled: false })
    expect(requestMicrophonePermission).not.toHaveBeenCalled()
  })

  it('clicking the switch marks the voice tip seen before the disable settings update', async () => {
    const calls: string[] = []
    const updateSettings = vi.fn((updates: Partial<GlobalSettings>) => {
      calls.push(`settings:${String(updates.voice?.enabled)}`)
    })
    const { button, root } = await renderVoicePane({
      voiceEnabled: true,
      markFeatureTipsSeen: (ids) => calls.push(`seen:${ids.join(',')}`),
      updateSettings,
      requestMicrophonePermission: vi.fn(async () => deniedMicrophoneResult)
    })

    await clickSwitch(button)
    root.unmount()

    expect(calls).toEqual(['seen:voice-dictation', 'settings:false'])
    expect(updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        voice: expect.objectContaining({ enabled: false })
      })
    )
    expect(window.api.developerPermissions.request).not.toHaveBeenCalled()
  })

  it('clicking the switch marks the voice tip seen before requesting microphone permission', async () => {
    const calls: string[] = []
    const updateSettings = vi.fn((updates: Partial<GlobalSettings>) => {
      calls.push(`settings:${String(updates.voice?.enabled)}`)
    })
    const { button, root } = await renderVoicePane({
      voiceEnabled: false,
      markFeatureTipsSeen: (ids) => calls.push(`seen:${ids.join(',')}`),
      updateSettings,
      requestMicrophonePermission: async () => {
        calls.push('permission-request')
        return deniedMicrophoneResult
      }
    })

    await clickSwitch(button)
    root.unmount()

    expect(calls).toEqual(['seen:voice-dictation', 'permission-request'])
    expect(updateSettings).not.toHaveBeenCalled()
  })

  it('marks the voice tip seen before requesting microphone permission when enabling is denied', async () => {
    const calls: string[] = []
    const updateVoiceSettings = vi.fn((updates: { enabled?: boolean }) => {
      calls.push(`settings:${String(updates.enabled)}`)
    })

    await handleVoiceDictationToggle({
      voiceEnabled: false,
      markFeatureTipsSeen: (ids) => calls.push(`seen:${ids.join(',')}`),
      updateVoiceSettings,
      requestMicrophonePermission: async () => {
        calls.push('permission-request')
        return deniedMicrophoneResult
      },
      setPermissionPending: (pending) => calls.push(`pending:${String(pending)}`),
      notifyPermissionRequired: () => calls.push('permission-required')
    })

    expect(calls).toEqual([
      'seen:voice-dictation',
      'pending:true',
      'permission-request',
      'permission-required',
      'pending:false'
    ])
    expect(updateVoiceSettings).not.toHaveBeenCalled()
  })

  it('does not record voice feature interaction from the settings switch', async () => {
    const recordFeatureInteraction = vi.fn()
    const { button, root } = await renderVoicePane({
      voiceEnabled: true,
      markFeatureTipsSeen: vi.fn(),
      updateSettings: vi.fn(),
      recordFeatureInteraction
    })

    await clickSwitch(button)
    root.unmount()

    expect(recordFeatureInteraction).not.toHaveBeenCalled()
  })

  it('merges an in-flight voice write onto the newest settings, not the render-time snapshot', async () => {
    const updateSettings = vi.fn()
    let resolveClear: () => void = () => {}
    const clearing = new Promise<{ configured: boolean }>((resolve) => {
      resolveClear = () => resolve({ configured: false })
    })
    useAppStoreMock.mockImplementation((selector: (state: Record<string, unknown>) => unknown) =>
      selector({
        modelStates: [],
        refreshModelStates: vi.fn(),
        markFeatureTipsSeen: vi.fn(),
        recordFeatureInteraction: vi.fn(),
        keybindings: {},
        keybindingSnapshot: null,
        setKeybindingOverride: vi.fn(async () => undefined),
        resetKeybindingOverride: vi.fn(async () => undefined)
      })
    )
    useShortcutLabelMock.mockReturnValue('Ctrl+Shift+Y')
    installWindowApi(vi.fn(async () => deniedMicrophoneResult))
    window.api.speech.getOpenAiApiKeyStatus = vi.fn(async () => ({ configured: true }))
    window.api.speech.clearOpenAiApiKey = vi.fn(() => clearing)

    const settingsWithKey = (enabled: boolean): GlobalSettings =>
      ({
        voice: {
          ...getDefaultVoiceSettings(),
          enabled,
          openAiApiKeyConfigured: true,
          microphoneDeviceId: 'usb-mic',
          microphoneDeviceLabel: 'USB Microphone'
        }
      }) as GlobalSettings

    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    await act(async () => {
      root.render(
        <TooltipProvider>
          <VoicePane settings={settingsWithKey(true)} updateSettings={updateSettings} />
        </TooltipProvider>
      )
    })

    const disconnect = container.querySelector<HTMLButtonElement>(
      'button[aria-label="Disconnect OpenAI API key"]'
    )
    if (!disconnect) {
      throw new Error('Disconnect OpenAI API key button was not rendered')
    }
    await act(async () => {
      disconnect.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })

    // The user turns dictation off while the clear-key IPC is still in flight.
    await act(async () => {
      root.render(
        <TooltipProvider>
          <VoicePane settings={settingsWithKey(false)} updateSettings={updateSettings} />
        </TooltipProvider>
      )
    })

    await act(async () => {
      resolveClear()
      await clearing
    })
    root.unmount()

    expect(updateSettings).toHaveBeenCalledTimes(1)
    expect(updateSettings).toHaveBeenCalledWith({
      voice: {
        ...getDefaultVoiceSettings(),
        enabled: false,
        openAiApiKeyConfigured: false,
        microphoneDeviceId: 'usb-mic',
        microphoneDeviceLabel: 'USB Microphone'
      }
    })
  })
})
