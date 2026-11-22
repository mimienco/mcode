import { expect, it, vi } from 'vitest'
import { createSettingsDocumentBridge } from './tauri-settings-document-client'

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

it('a stale read cannot overwrite or announce a newer persisted settings snapshot', async () => {
  const read = deferred<unknown>()
  const invoke = vi.fn((command: string) =>
    command === 'settings_get_document'
      ? read.promise
      : Promise.resolve({ settings: { theme: 'light' } })
  )
  const bridge = createSettingsDocumentBridge(invoke)
  const changed = vi.fn()
  bridge.onDocumentChanged(changed)
  const initial = bridge.getDocument()
  await Promise.resolve()
  await bridge.setDocument({ settings: { theme: 'light' } })
  read.resolve({ settings: { theme: 'dark' } })
  await initial
  expect(bridge.cache()).toEqual({ settings: { theme: 'light' } })
  expect(changed).toHaveBeenCalledTimes(1)
})

it('serializes writes before reading so rapid settings updates retain invocation order', async () => {
  const first = deferred<unknown>()
  const invoke = vi
    .fn()
    .mockReturnValueOnce(first.promise)
    .mockResolvedValueOnce({ settings: { theme: 'light' } })
    .mockResolvedValueOnce({ settings: { theme: 'light' } })
  const bridge = createSettingsDocumentBridge(invoke)
  const older = bridge.setDocument({ settings: { theme: 'dark' } })
  const newer = bridge.setDocument({ settings: { theme: 'light' } })
  const read = bridge.getDocument()
  await Promise.resolve()
  expect(invoke).toHaveBeenCalledTimes(1)
  first.resolve({ settings: { theme: 'dark' } })
  await Promise.all([older, newer, read])
  expect(invoke.mock.calls).toEqual([
    ['settings_set_document', { patch: { settings: { theme: 'dark' } } }],
    ['settings_set_document', { patch: { settings: { theme: 'light' } } }],
    ['settings_get_document', undefined]
  ])
  expect(bridge.cache()).toEqual({ settings: { theme: 'light' } })
})
