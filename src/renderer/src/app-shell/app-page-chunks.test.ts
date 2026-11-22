import { describe, expect, it, vi } from 'vitest'
import { createAppPagePrefetcher, type PrefetchableAppPage } from './app-page-chunks'

function makeImporters(settingsImporter: () => Promise<unknown>) {
  const fallback = vi.fn(async () => ({}))
  return {
    activity: fallback,
    artifacts: fallback,
    automations: fallback,
    mobile: fallback,
    settings: settingsImporter,
    skills: fallback,
    space: fallback,
    tasks: fallback
  } satisfies Record<PrefetchableAppPage, () => Promise<unknown>>
}

describe('createAppPagePrefetcher', () => {
  it('deduplicates repeated intent for the same route', async () => {
    const settingsImporter = vi.fn(async () => ({}))
    const prefetch = createAppPagePrefetcher(makeImporters(settingsImporter))

    await Promise.all([prefetch('settings'), prefetch('settings')])

    expect(settingsImporter).toHaveBeenCalledOnce()
  })

  it('allows a retry after a transient prefetch failure', async () => {
    const settingsImporter = vi
      .fn<() => Promise<unknown>>()
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValueOnce({})
    const prefetch = createAppPagePrefetcher(makeImporters(settingsImporter))

    await expect(prefetch('settings')).rejects.toThrow('offline')
    await expect(prefetch('settings')).resolves.toEqual({})
    expect(settingsImporter).toHaveBeenCalledTimes(2)
  })
})
