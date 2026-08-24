import { beforeEach, describe, expect, it, vi } from 'vitest'

const listQuickOpenFilesMock = vi.hoisted(() => vi.fn())

vi.mock('../ipc/filesystem-list-files', () => ({
  listQuickOpenFiles: listQuickOpenFilesMock
}))

import { createRuntimeFileCommands } from './mcode-runtime-files-test-harness'
import { MOBILE_FILE_LIST_LIMIT } from './mcode-runtime-files'

// PERF-05: the mobile file listing used to enumerate the whole tree and sort
// everything for at most MOBILE_FILE_LIST_LIMIT rows. It must now request one
// past the limit so truncation stays detectable without full-tree scans.
describe('listMobileFiles bounds', () => {
  let commands: ReturnType<typeof createRuntimeFileCommands>['commands']

  beforeEach(() => {
    listQuickOpenFilesMock.mockReset()
    ;({ commands } = createRuntimeFileCommands())
  })

  function makePaths(count: number): string[] {
    return Array.from({ length: count }, (_, index) =>
      index % 2 === 0 ? `src/file-${index}.ts` : `docs/doc-${index}.md`
    )
  }

  it('requests exactly limit+1 entries from the local listing', async () => {
    listQuickOpenFilesMock.mockResolvedValue(makePaths(MOBILE_FILE_LIST_LIMIT))

    await commands.listMobileFiles('wt-1')

    expect(listQuickOpenFilesMock).toHaveBeenCalledTimes(1)
    expect(listQuickOpenFilesMock.mock.calls[0][4]).toBe(MOBILE_FILE_LIST_LIMIT + 1)
  })

  it('truncates to the limit and flags a non-exact total when more arrive', async () => {
    const paths = makePaths(MOBILE_FILE_LIST_LIMIT + 1)
    listQuickOpenFilesMock.mockResolvedValue(paths)

    const result = await commands.listMobileFiles('wt-1')

    expect(result.files).toHaveLength(MOBILE_FILE_LIST_LIMIT)
    expect(result.truncated).toBe(true)
    expect(result.totalCount).toBeLessThanOrEqual(MOBILE_FILE_LIST_LIMIT + 1)
    expect(result.totalCountExact).toBe(false)
  })

  it('marks the total exact when everything fits and sorts ascending', async () => {
    const paths = ['src/b.ts', 'src/a.ts', 'docs/c.md']
    listQuickOpenFilesMock.mockResolvedValue(paths)

    const result = await commands.listMobileFiles('wt-1')

    expect(result.files.map((entry) => entry.relativePath)).toEqual([
      'docs/c.md',
      'src/a.ts',
      'src/b.ts'
    ])
    expect(result.truncated).toBe(false)
    expect(result.totalCountExact).toBe(true)
  })
})
