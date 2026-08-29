import { expect, it, vi } from 'vitest'
import { createRuntimeDesktopAppLifecycleClient } from './runtime-desktop-app-lifecycle-client'

it('prepares renderer state before requesting a Tauri restart', async () => {
  const calls: string[] = []
  const requestShutdown = vi.fn(async () => {
    calls.push('request')
    return { version: 1 as const, requestId: '13', intent: 'restart' as const }
  })
  const client = createRuntimeDesktopAppLifecycleClient({
    isTauri: () => true,
    electron: { restart: vi.fn(), relaunch: vi.fn() },
    loadTauri: async () => ({ requestShutdown }),
    prepareTauriRestart: async () => {
      calls.push('prepare')
    },
    abortTauriRestart: vi.fn()
  })

  await client.relaunch()

  expect(calls).toEqual(['prepare', 'request'])
  expect(requestShutdown).toHaveBeenCalledWith('restart')
})

it('clears restart preparation when the Tauri request is rejected', async () => {
  const failure = new Error('shutdown unavailable')
  const abortTauriRestart = vi.fn()
  const client = createRuntimeDesktopAppLifecycleClient({
    isTauri: () => true,
    electron: { restart: vi.fn(), relaunch: vi.fn() },
    loadTauri: async () => ({
      requestShutdown: () => Promise.reject(failure)
    }),
    prepareTauriRestart: () => Promise.resolve(),
    abortTauriRestart
  })

  await expect(client.restart()).rejects.toBe(failure)

  expect(abortTauriRestart).toHaveBeenCalledOnce()
})

it('keeps Electron restart and relaunch semantics on the fallback runtime', async () => {
  const electron = {
    restart: vi.fn(() => Promise.resolve()),
    relaunch: vi.fn(() => Promise.resolve())
  }
  const loadTauri = vi.fn()
  const client = createRuntimeDesktopAppLifecycleClient({
    isTauri: () => false,
    electron,
    loadTauri,
    prepareTauriRestart: vi.fn(),
    abortTauriRestart: vi.fn()
  })

  await client.restart()
  await client.relaunch()

  expect(electron.restart).toHaveBeenCalledOnce()
  expect(electron.relaunch).toHaveBeenCalledOnce()
  expect(loadTauri).not.toHaveBeenCalled()
})
