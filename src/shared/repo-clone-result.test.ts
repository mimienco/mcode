import { describe, expect, it } from 'vitest'
import type { Repo } from './repo-types'
import { unwrapLocalRepoCloneResult } from './repo-clone-result'

describe('unwrapLocalRepoCloneResult', () => {
  it('returns successful clone results unchanged', () => {
    const repo = { id: 'repo-1', path: '/repo', displayName: 'repo' } as Repo
    expect(unwrapLocalRepoCloneResult(repo)).toBe(repo)
  })

  it('rethrows expected result errors on the renderer side', () => {
    expect(() => unwrapLocalRepoCloneResult({ error: 'Clone failed: bad URL' })).toThrow(
      'Clone failed: bad URL'
    )
  })
})
