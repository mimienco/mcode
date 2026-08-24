import { beforeEach, describe, expect, it, vi } from 'vitest'
import { handlers, store, resetFilesystemIpcMocks } from './filesystem-test-harness'

vi.mock('electron', async () => (await import('./filesystem-test-harness')).electronMock)
vi.mock('fs/promises', async () => (await import('./filesystem-test-harness')).fsPromisesMock)
vi.mock(
  '../wsl-unc-delete',
  async () => (await import('./filesystem-test-harness')).wslUncDeleteMock
)
vi.mock(
  '../crash-reporting/crash-breadcrumb-store',
  async () => (await import('./filesystem-test-harness')).crashBreadcrumbMock
)
vi.mock(
  '../local-downloaded-folder-promotion',
  async () => (await import('./filesystem-test-harness')).folderPromotionMock
)
vi.mock(
  '../git/status',
  async () => (await import('./filesystem-test-harness')).gitStatusModuleMock
)
vi.mock(
  '../git/check-ignored-paths',
  async () => (await import('./filesystem-test-harness')).gitIgnoredPathsMock
)
vi.mock('../git/worktree', async () => (await import('./filesystem-test-harness')).gitWorktreeMock)
vi.mock(
  '../providers/ssh-filesystem-dispatch',
  async () => (await import('./filesystem-test-harness')).sshFilesystemDispatchMock
)
vi.mock(
  '../providers/ssh-git-dispatch',
  async () => (await import('./filesystem-test-harness')).sshGitDispatchMock
)

const listQuickOpenFilesMock = vi.hoisted(() => vi.fn())
const searchQuickOpenFilePathsMock = vi.hoisted(() => vi.fn())

vi.mock('./filesystem-list-files', () => ({
  listQuickOpenFiles: listQuickOpenFilesMock
}))
vi.mock('./filesystem-search-file-paths', () => ({
  searchQuickOpenFilePaths: searchQuickOpenFilePathsMock
}))

import { registerFilesystemHandlers } from './filesystem'
import { invalidateAuthorizedRootsCache } from './registered-worktree-roots-cache'

// PERF-04: the local fs:listFiles branch must forward maxResults and route
// searchQuery/searchMatchMode to the bounded local ranker instead of running
// an unbounded full-tree scan.
describe('fs:listFiles local Quick Open bounds', () => {
  beforeEach(() => {
    resetFilesystemIpcMocks()
    invalidateAuthorizedRootsCache()
    listQuickOpenFilesMock.mockReset()
    searchQuickOpenFilePathsMock.mockReset()
  })

  it('forwards maxResults on the local path so replies stay bounded', async () => {
    listQuickOpenFilesMock.mockResolvedValue(['src/a.ts'])
    registerFilesystemHandlers(store as never)

    await handlers.get('fs:listFiles')!(null, {
      rootPath: '/home/user/repo',
      maxResults: 100
    })

    expect(listQuickOpenFilesMock).toHaveBeenCalledTimes(1)
    const call = listQuickOpenFilesMock.mock.calls[0]
    expect(call[0]).toBe('/home/user/repo')
    expect(call[4]).toBe(100)
  })

  it('routes local searchQuery through the bounded ranker with limit and match mode', async () => {
    searchQuickOpenFilePathsMock.mockResolvedValue({
      paths: ['src/target.ts'],
      totalCount: 1,
      truncated: false
    })
    registerFilesystemHandlers(store as never)

    const result = await handlers.get('fs:listFiles')!(null, {
      rootPath: '/home/user/repo',
      maxResults: 100,
      searchQuery: 'target',
      searchMatchMode: 'substring'
    })

    expect(listQuickOpenFilesMock).not.toHaveBeenCalled()
    expect(searchQuickOpenFilePathsMock).toHaveBeenCalledTimes(1)
    const [, , options] = searchQuickOpenFilePathsMock.mock.calls[0]
    expect(options.query).toBe('target')
    expect(options.limit).toBe(100)
    expect(options.matchMode).toBe('substring')
    expect(result).toEqual(['src/target.ts'])
  })
})
