import { describe, expect, it, vi } from 'vitest'

import { createTaskPageDetailPrefetcher } from './task-page-detail-chunks'

function createImporters(githubItem: () => Promise<unknown>) {
  const fallback = vi.fn(async () => ({ default: {} }))
  return {
    githubItem,
    githubPullRequest: fallback,
    githubProject: fallback,
    gitlabItem: fallback,
    jiraIssue: fallback,
    jiraConnect: fallback,
    linearIssue: fallback,
    linearConnect: fallback
  }
}

describe('task page detail chunk prefetcher', () => {
  it('deduplicates repeated intent for the same detail surface', async () => {
    const githubItem = vi.fn(async () => ({ default: {} }))
    const prefetch = createTaskPageDetailPrefetcher(createImporters(githubItem))

    const first = prefetch('githubItem')
    const second = prefetch('githubItem')

    expect(first).toBe(second)
    await first
    expect(githubItem).toHaveBeenCalledTimes(1)
  })

  it('allows a later interaction to retry a rejected import', async () => {
    const githubItem = vi
      .fn<() => Promise<unknown>>()
      .mockRejectedValueOnce(new Error('update race'))
      .mockResolvedValueOnce({ default: {} })
    const prefetch = createTaskPageDetailPrefetcher(createImporters(githubItem))

    await expect(prefetch('githubItem')).rejects.toThrow('update race')
    await expect(prefetch('githubItem')).resolves.toEqual({ default: {} })
    expect(githubItem).toHaveBeenCalledTimes(2)
  })
})
