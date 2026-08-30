import { expect, it, vi } from 'vitest'
import type { DesktopInvoke } from './desktop-client'
import {
  createTauriDesktopUpdaterCapability,
  mapNativeUpdateStatus,
  type NativeUpdateStatus
} from './tauri-desktop-updater-capability'

function createCapability(invoke: DesktopInvoke, version = '1.4.178') {
  return createTauriDesktopUpdaterCapability(
    invoke,
    vi.fn(async () => () => undefined),
    async () => version,
    vi.fn()
  )
}

it('checks the running release channel through the narrow native command', async () => {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const response: NativeUpdateStatus = {
    state: 'not-available',
    channel: 'hourly',
    userInitiated: true
  }
  const capability = createCapability(async (command, args) => {
    calls.push({ command, args })
    return response
  }, '1.4.178-hourly.202608301530')

  await capability.check()

  expect(calls).toEqual([
    {
      command: 'update_check',
      args: { request: { channel: 'hourly', userInitiated: true } }
    }
  ])
})

it('marks the automatic launch check as non-user-initiated', async () => {
  const invoke = vi.fn(async () => ({
    state: 'not-available',
    channel: 'stable',
    userInitiated: false
  }))
  const capability = createCapability(invoke)

  await capability.checkOnLaunch?.()

  expect(invoke).toHaveBeenCalledWith('update_check', {
    request: { channel: 'stable', userInitiated: false }
  })
})

it('honors an explicit channel and rejects Electron-only check modes', async () => {
  const invoke = vi.fn(async () => ({
    state: 'not-available',
    channel: 'daily',
    userInitiated: true
  }))
  const capability = createCapability(invoke)

  await capability.check({ channel: 'daily' })
  await expect(capability.check({ targetTag: 'v1.4.177' })).rejects.toThrow(
    'Pinned update builds are not available through the native updater yet.'
  )
  await expect(capability.check({ localBuild: true })).rejects.toThrow(
    'Local update builds are not available through the native updater yet.'
  )

  expect(invoke).toHaveBeenCalledOnce()
})

it('maps native progress and error states to the existing renderer contract', () => {
  expect(
    mapNativeUpdateStatus({
      state: 'downloading',
      channel: 'adhoc',
      version: '1.4.179-adhoc.20260830153001',
      downloadedBytes: 50,
      totalBytes: 100,
      percent: 50
    })
  ).toEqual({
    state: 'downloading',
    version: '1.4.179-adhoc.20260830153001',
    percent: 50,
    source: 'adhoc'
  })
  expect(
    mapNativeUpdateStatus({
      state: 'error',
      operation: 'install',
      code: 'native_update_install_failed',
      message: 'signature rejected',
      retryable: true,
      channel: 'stable',
      version: '1.4.179',
      userInitiated: true
    })
  ).toEqual({ state: 'error', message: 'signature rejected', userInitiated: true })
})

it('refuses malformed native responses instead of casting them into UI state', async () => {
  const capability = createCapability(async () => ({
    state: 'downloaded',
    channel: 'stable'
  }))

  await expect(capability.getStatus()).rejects.toThrow('Invalid update_get_status response')
})

it('downloads and requests lifecycle-owned installation through exact commands', async () => {
  const calls: string[] = []
  const invoke: DesktopInvoke = async (command) => {
    calls.push(command)
    if (command === 'update_download') {
      return { state: 'downloaded', channel: 'stable', version: '1.4.179' }
    }
    return { version: 1, requestId: '17', intent: 'update_install' }
  }
  const capability = createCapability(invoke)

  await capability.download()
  await capability.quitAndInstall()

  expect(calls).toEqual(['update_download', 'update_install'])
})

it('validates updater events before delivering them to renderer state', async () => {
  const eventListeners = new Map<string, (event: { payload: unknown }) => void>()
  const listener = vi.fn()
  const capability = createTauriDesktopUpdaterCapability(
    vi.fn(),
    async (event, callback) => {
      eventListeners.set(event, callback)
      return () => undefined
    },
    async () => '1.4.178',
    vi.fn()
  )

  await capability.onStatus(listener)
  const statusListener = eventListeners.get('mcode://updater/status')
  statusListener?.({ payload: { state: 'downloaded', channel: 'stable' } })
  statusListener?.({
    payload: { state: 'downloaded', channel: 'stable', version: '1.4.179' }
  })

  expect(listener).toHaveBeenCalledOnce()
  expect(listener).toHaveBeenCalledWith({ state: 'downloaded', version: '1.4.179' })
})

it('releases renderer restart preparation when native installation fails', async () => {
  const eventListeners = new Map<string, (event: { payload: unknown }) => void>()
  const onInstallAborted = vi.fn()
  const capability = createTauriDesktopUpdaterCapability(
    vi.fn(),
    async (event, callback) => {
      eventListeners.set(event, callback)
      return () => undefined
    },
    async () => '1.4.178',
    onInstallAborted
  )
  await capability.onStatus(vi.fn())

  eventListeners.get('mcode://updater/status')?.({
    payload: {
      state: 'error',
      operation: 'install',
      code: 'native_update_install_failed',
      message: 'installer rejected package',
      retryable: true,
      channel: 'stable',
      version: '1.4.179',
      userInitiated: true
    }
  })

  expect(onInstallAborted).toHaveBeenCalledOnce()
})

it('releases renderer restart preparation when native install acknowledgement times out', async () => {
  const eventListeners = new Map<string, (event: { payload: unknown }) => void>()
  const onInstallAborted = vi.fn()
  const capability = createTauriDesktopUpdaterCapability(
    vi.fn(),
    async (event, callback) => {
      eventListeners.set(event, callback)
      return () => undefined
    },
    async () => '1.4.178',
    onInstallAborted
  )
  await capability.onStatus(vi.fn())

  eventListeners.get('mcode://updater/install-aborted')?.({
    payload: { version: 1, requestId: '18', intent: 'update_install' }
  })

  expect(onInstallAborted).toHaveBeenCalledOnce()
})
