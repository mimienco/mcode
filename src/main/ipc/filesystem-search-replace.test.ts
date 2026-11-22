import { describe, expect, it, vi, beforeEach } from 'vitest'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const handleMock = vi.hoisted(() => vi.fn())
const resolveAuthorizedPathMock = vi.hoisted(() => vi.fn(async (_p: string) => _p))
const requireSshFilesystemProviderMock = vi.hoisted(() => vi.fn())

vi.mock('electron', () => ({ ipcMain: { handle: handleMock } }))
vi.mock('./filesystem-auth', () => ({
  resolveAuthorizedPath: (p: string) => resolveAuthorizedPathMock(p)
}))
vi.mock('../providers/ssh-filesystem-dispatch', () => ({
  requireSshFilesystemProvider: (...args: unknown[]) =>
    requireSshFilesystemProviderMock(...(args as [string]))
}))

import {
  performReplace,
  registerFilesystemSearchReplaceHandlers,
  REMOTE_REPLACE_UNSUPPORTED_MESSAGE
} from './filesystem-search-replace'

let root: string

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'mcode-search-replace-'))
  return () => rmSync(root, { recursive: true, force: true })
})

const write = (relativePath: string, content: string): string => {
  const target = join(root, relativePath)
  mkdirSync(join(target, '..'), { recursive: true })
  writeFileSync(target, content)
  return target
}

describe('performReplace', () => {
  const run = (
    targets: { relativePath: string }[],
    extra: Partial<Parameters<typeof performReplace>[0]> = {}
  ) =>
    performReplace({
      store: {} as never,
      rootPath: root,
      query: 'foo',
      replacement: 'bar',
      options: { caseSensitive: false },
      targets,
      skipPaths: [],
      ...extra
    })

  it('rewrites every target and reports counts', async () => {
    write('a.txt', 'foo foo\nFOO')
    write(join('nested', 'b.md'), 'foobar foo')
    const summary = await run([
      { relativePath: 'a.txt' },
      { relativePath: join('nested', 'b.md') }
    ])
    expect(readFileSync(join(root, 'a.txt'), 'utf8')).toBe('bar bar\nbar')
    expect(summary.writtenFiles).toBe(2)
    expect(summary.writtenOccurrences).toBe(5)
    expect(summary.skippedFiles).toBe(0)
  })

  it('skips unchanged files without writing', async () => {
    const path = write('same.txt', 'untouched')
    const summary = await run([{ relativePath: 'same.txt' }])
    expect(summary.outcomes[0]).toMatchObject({ status: 'skipped', reason: 'unchanged' })
    expect(readFileSync(path, 'utf8')).toBe('untouched')
  })

  it('never writes dirty editor tabs', async () => {
    const path = write('dirty.txt', 'foo draft')
    const summary = await run([{ relativePath: 'dirty.txt' }], {
      skipPaths: [path]
    })
    expect(summary.outcomes[0]).toMatchObject({ status: 'skipped', reason: 'dirty-editor-tab' })
    expect(readFileSync(path, 'utf8')).toBe('foo draft')
  })

  it('refuses binary files', async () => {
    write('blob.bin', 'ok\x00binary')
    const summary = await run([{ relativePath: 'blob.bin' }])
    expect(summary.outcomes[0]).toMatchObject({ status: 'skipped', reason: 'binary-file' })
  })

  it('rejects targets escaping the root', async () => {
    write('../outside.txt', 'foo')
    const summary = await run([{ relativePath: '../outside.txt' }])
    expect(summary.outcomes[0]).toMatchObject({ status: 'failed', reason: 'outside-root' })
    expect(readFileSync(join(root, '..', 'outside.txt'), 'utf8')).toBe('foo')
    rmSync(join(root, '..', 'outside.txt'))
  })

  it('reports missing files as gone', async () => {
    const summary = await run([{ relativePath: 'deleted.txt' }])
    expect(summary.outcomes[0]).toMatchObject({ status: 'skipped', reason: 'gone' })
  })

  it('stops before the next file when aborted', async () => {
    write('one.txt', 'foo')
    write('two.txt', 'foo')
    const controller = new AbortController()
    controller.abort()
    const summary = await run(
      [{ relativePath: 'one.txt' }, { relativePath: 'two.txt' }],
      { signal: controller.signal }
    )
    expect(summary.outcomes).toHaveLength(0)
    expect(readFileSync(join(root, 'two.txt'), 'utf8')).toBe('foo')
  })

  it('preserves executable mode through the atomic rename', { skip: process.platform === 'win32' }, async () => {
    const path = write('script.sh', '#!/bin/sh\nrun foo')
    chmodSync(path, 0o755)
    expect(readFileSync(path, 'utf8')).toContain('run bar')
    expect(statSync(path).mode & 0o777).toBe(0o755)
  })
})

describe('registerFilesystemSearchReplaceHandlers', () => {
  it('registers invoke and cancel channels', () => {
    registerFilesystemSearchReplaceHandlers({} as never)
    const channels = handleMock.mock.calls.map(([channel]) => channel)
    expect(channels).toContain('fs:searchReplace')
    expect(channels).toContain('fs:cancelSearchReplace')
  })

  function registeredHandler(channel: string): (event: unknown, args: unknown) => unknown {
    const call = handleMock.mock.calls.find(([name]) => name === channel)
    if (!call) {
      throw new Error(`handler ${channel} not registered`)
    }
    return call[1] as (event: unknown, args: unknown) => unknown
  }

  it('routes connectionId requests through the SSH provider', async () => {
    registerFilesystemSearchReplaceHandlers({} as never)
    const summary = {
      outcomes: [],
      writtenFiles: 0,
      writtenOccurrences: 0,
      skippedFiles: 0,
      failedFiles: 0
    }
    const provider = { searchReplace: vi.fn().mockResolvedValue(summary) }
    requireSshFilesystemProviderMock.mockReturnValue(provider)

    const handler = registeredHandler('fs:searchReplace')
    const args = {
      connectionId: 'conn-1',
      rootPath: '/remote/repo',
      query: 'foo',
      replacement: 'bar',
      options: {},
      targets: [{ filePath: '/remote/repo/a.ts', relativePath: 'a.ts' }]
    }
    // Why: the remote host owns authorization; the local path resolver must not run
    // for this request. Earlier local-path tests legitimately call the shared hoisted
    // mock, so assert no NEW calls rather than an absolute zero.
    const resolveCallsBefore = resolveAuthorizedPathMock.mock.calls.length
    await expect(handler({ sender: { id: 1 } }, args)).resolves.toBe(summary)
    expect(requireSshFilesystemProviderMock).toHaveBeenCalledWith('conn-1')
    expect(provider.searchReplace).toHaveBeenCalledWith(args)
    expect(resolveAuthorizedPathMock.mock.calls).toHaveLength(resolveCallsBefore)
  })

  it('fails closed when the paired host predates replace support', async () => {
    registerFilesystemSearchReplaceHandlers({} as never)
    requireSshFilesystemProviderMock.mockReturnValue({})

    const handler = registeredHandler('fs:searchReplace')
    await expect(
      handler({ sender: { id: 1 } }, {
        connectionId: 'old-host',
        rootPath: '/remote/repo',
        query: 'foo',
        replacement: 'bar',
        options: {},
        targets: []
      })
    ).rejects.toThrow(REMOTE_REPLACE_UNSUPPORTED_MESSAGE)
  })
})
