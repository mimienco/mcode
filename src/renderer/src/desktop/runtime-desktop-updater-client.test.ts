import { expect, it, vi } from 'vitest'
import type { UpdateStatus } from '../../../shared/update-status-types'
import type { DesktopUpdaterCoreCapability } from './tauri-desktop-updater-capability'
import { createRuntimeDesktopUpdaterClient } from './runtime-desktop-updater-client'

function updater(label: string): DesktopUpdaterCoreCapability {
  return {
    getVersion: vi.fn(async () => label),
    getStatus: vi.fn(async (): Promise<UpdateStatus> => ({ state: 'idle' })),
    check: vi.fn(async () => undefined),
    download: vi.fn(async () => undefined),
    quitAndInstall: vi.fn(async () => undefined),
    onStatus: vi.fn(async () => () => undefined)
  }
}

it('lazily resolves and caches the native updater only in Tauri', async () => {
  const electron = updater('electron')
  const tauri = updater('tauri')
  const loadTauri = vi.fn(async () => tauri)
  const client = createRuntimeDesktopUpdaterClient({
    isTauri: () => true,
    electron,
    loadTauri,
    prepareTauriInstall: vi.fn(),
    abortTauriInstall: vi.fn()
  })

  expect(await client.getVersion()).toBe('tauri')
  await client.download()

  expect(loadTauri).toHaveBeenCalledOnce()
  expect(tauri.download).toHaveBeenCalledOnce()
  expect(electron.download).not.toHaveBeenCalled()
})

it('preserves the Electron updater without loading Tauri code', async () => {
  const electron = updater('electron')
  const loadTauri = vi.fn()
  const client = createRuntimeDesktopUpdaterClient({
    isTauri: () => false,
    electron,
    loadTauri,
    prepareTauriInstall: vi.fn(),
    abortTauriInstall: vi.fn()
  })

  await client.check({ includePrerelease: true })
  await client.checkOnLaunch?.()
  await client.quitAndInstall()

  expect(electron.check).toHaveBeenCalledWith({ includePrerelease: true })
  expect(electron.quitAndInstall).toHaveBeenCalledOnce()
  expect(loadTauri).not.toHaveBeenCalled()
})

it('routes the automatic launch check only to the native updater', async () => {
  const electron = updater('electron')
  const tauri = updater('tauri')
  tauri.checkOnLaunch = vi.fn(async () => undefined)
  const client = createRuntimeDesktopUpdaterClient({
    isTauri: () => true,
    electron,
    loadTauri: async () => tauri,
    prepareTauriInstall: vi.fn(),
    abortTauriInstall: vi.fn()
  })

  await client.checkOnLaunch?.()

  expect(tauri.checkOnLaunch).toHaveBeenCalledOnce()
  expect(electron.check).not.toHaveBeenCalled()
})

it('routes status subscription ownership through the selected runtime', async () => {
  const electron = updater('electron')
  const tauri = updater('tauri')
  const listener = vi.fn()
  const client = createRuntimeDesktopUpdaterClient({
    isTauri: () => true,
    electron,
    loadTauri: async () => tauri,
    prepareTauriInstall: vi.fn(),
    abortTauriInstall: vi.fn()
  })

  await client.onStatus(listener)

  expect(tauri.onStatus).toHaveBeenCalledWith(listener)
  expect(electron.onStatus).not.toHaveBeenCalled()
})

it('prepares Tauri install shutdown and rolls preparation back on request failure', async () => {
  const failure = new Error('native request failed')
  const tauri = updater('tauri')
  vi.mocked(tauri.quitAndInstall).mockRejectedValue(failure)
  const prepareTauriInstall = vi.fn(async () => undefined)
  const abortTauriInstall = vi.fn()
  const client = createRuntimeDesktopUpdaterClient({
    isTauri: () => true,
    electron: updater('electron'),
    loadTauri: async () => tauri,
    prepareTauriInstall,
    abortTauriInstall
  })

  await expect(client.quitAndInstall()).rejects.toBe(failure)

  expect(prepareTauriInstall).toHaveBeenCalledOnce()
  expect(abortTauriInstall).toHaveBeenCalledOnce()
})
