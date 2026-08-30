import { expect, it, vi } from 'vitest'
import { createRuntimeDesktopCloseToTrayClient } from './runtime-desktop-close-to-tray-client'

it('persists and sends close-to-tray preference changes to the Tauri host', async () => {
  const setEnabled = vi.fn(() => Promise.resolve())
  const loadTauri = vi.fn(() => Promise.resolve({ setEnabled }))
  const persist = vi.fn(() => Promise.resolve())
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => true,
    loadTauri
  })

  await client.updatePreference(true, false, persist)

  expect(persist).toHaveBeenCalledWith(true)
  expect(loadTauri).toHaveBeenCalledOnce()
  expect(setEnabled).toHaveBeenCalledWith(true)
})

it('persists without loading the native client for the Electron host', async () => {
  const loadTauri = vi.fn()
  const persist = vi.fn(() => Promise.resolve())
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => false,
    loadTauri
  })

  await client.updatePreference(true, false, persist)

  expect(persist).toHaveBeenCalledWith(true)
  expect(loadTauri).not.toHaveBeenCalled()
})

it('does not touch the native host when renderer persistence fails', async () => {
  const setEnabled = vi.fn(() => Promise.resolve())
  const loadTauri = vi.fn(() => Promise.resolve({ setEnabled }))
  const persistError = new Error('renderer persistence failed')
  const persist = vi.fn(() => Promise.reject(persistError))
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => true,
    loadTauri
  })

  await expect(client.updatePreference(true, false, persist)).rejects.toBe(persistError)

  expect(loadTauri).not.toHaveBeenCalled()
  expect(setEnabled).not.toHaveBeenCalled()
})

it('rolls renderer persistence back when the native update fails', async () => {
  const nativeError = new Error('native update failed')
  const setEnabled = vi.fn(() => Promise.reject(nativeError))
  const persist = vi.fn(() => Promise.resolve())
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => true,
    loadTauri: () => Promise.resolve({ setEnabled })
  })

  await expect(client.updatePreference(true, false, persist)).rejects.toBe(nativeError)

  expect(persist.mock.calls).toEqual([[true], [false]])
})

it('reports both native and rollback failures', async () => {
  const nativeError = new Error('native update failed')
  const rollbackError = new Error('rollback failed')
  const setEnabled = vi.fn(() => Promise.reject(nativeError))
  const persist = vi
    .fn<(_: boolean) => Promise<void>>()
    .mockResolvedValueOnce()
    .mockRejectedValueOnce(rollbackError)
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => true,
    loadTauri: () => Promise.resolve({ setEnabled })
  })

  await expect(client.updatePreference(true, false, persist)).rejects.toEqual(
    new AggregateError(
      [nativeError, rollbackError],
      'Failed to apply and roll back the close-to-tray preference'
    )
  )
})

it('serializes overlapping preference transactions in request order', async () => {
  let finishFirstNativeUpdate: (() => void) | undefined
  const firstNativeUpdate = new Promise<void>((resolve) => {
    finishFirstNativeUpdate = resolve
  })
  const setEnabled = vi.fn((enabled: boolean) => (enabled ? firstNativeUpdate : Promise.resolve()))
  const persist = vi.fn(() => Promise.resolve())
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => true,
    loadTauri: () => Promise.resolve({ setEnabled })
  })

  const enable = client.updatePreference(true, false, persist)
  const disable = client.updatePreference(false, true, persist)
  await vi.waitFor(() => expect(setEnabled).toHaveBeenCalledTimes(1))

  expect(persist.mock.calls).toEqual([[true]])
  expect(setEnabled.mock.calls).toEqual([[true]])

  finishFirstNativeUpdate?.()
  await Promise.all([enable, disable])

  expect(persist.mock.calls).toEqual([[true], [false]])
  expect(setEnabled.mock.calls).toEqual([[true], [false]])
})

it('rolls overlapping consecutive failures back to the last effective value', async () => {
  const firstError = new Error('enable failed')
  const secondError = new Error('disable failed')
  const setEnabled = vi
    .fn<(_: boolean) => Promise<void>>()
    .mockRejectedValueOnce(firstError)
    .mockRejectedValueOnce(secondError)
  const persist = vi.fn(() => Promise.resolve())
  const client = createRuntimeDesktopCloseToTrayClient({
    isTauri: () => true,
    loadTauri: () => Promise.resolve({ setEnabled })
  })

  const enable = client.updatePreference(true, false, persist)
  const disable = client.updatePreference(false, true, persist)

  await expect(enable).rejects.toBe(firstError)
  await expect(disable).rejects.toBe(secondError)

  expect(persist.mock.calls).toEqual([[true], [false], [false], [false]])
  expect(setEnabled.mock.calls).toEqual([[true], [false]])
})
