import { describe, expect, it, vi } from 'vitest'

import { createAutomationEditorDialogPrefetcher } from './automation-editor-dialog-chunk'

describe('createAutomationEditorDialogPrefetcher', () => {
  it('shares one import across repeated pointer and focus intent', async () => {
    const module = { default: vi.fn() }
    const importer = vi.fn(async () => module)
    const prefetch = createAutomationEditorDialogPrefetcher(importer)

    const pointerRequest = prefetch()
    const focusRequest = prefetch()

    expect(focusRequest).toBe(pointerRequest)
    await expect(pointerRequest).resolves.toBe(module)
    expect(importer).toHaveBeenCalledOnce()
  })

  it('allows the next intent to retry after a transient import failure', async () => {
    const module = { default: vi.fn() }
    const importer = vi
      .fn<() => Promise<typeof module>>()
      .mockRejectedValueOnce(new Error('chunk raced an update'))
      .mockResolvedValueOnce(module)
    const prefetch = createAutomationEditorDialogPrefetcher(importer)

    await expect(prefetch()).rejects.toThrow('chunk raced an update')
    await expect(prefetch()).resolves.toBe(module)
    expect(importer).toHaveBeenCalledTimes(2)
  })
})
