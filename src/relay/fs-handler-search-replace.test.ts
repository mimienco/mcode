import { describe, expect, it, beforeEach } from 'vitest'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { searchReplaceOnRelay } from './fs-handler-search-replace'

let root: string

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'mcode-relay-replace-'))
  return () => rmSync(root, { recursive: true, force: true })
})

const write = (relativePath: string, content: string): string => {
  const target = join(root, relativePath)
  mkdirSync(join(target, '..'), { recursive: true })
  writeFileSync(target, content)
  return target
}

const invoke = (targets: string[], skipPaths: string[] = []) =>
  searchReplaceOnRelay({
    rootPath: root,
    query: 'foo',
    replacement: 'bar',
    options: { caseSensitive: false },
    targets: targets.map((relativePath) => ({ relativePath })),
    skipPaths
  })

describe('searchReplaceOnRelay', () => {
  it('rewrites targets with identical outcomes to the local engine', async () => {
    write('src/a.ts', 'foo FOO')
    write('docs/b.md', 'no match here')
    const summary = await invoke(['src/a.ts', 'docs/b.md'])
    expect(readFileSync(join(root, 'src/a.ts'), 'utf8')).toBe('bar bar')
    expect(summary.writtenFiles).toBe(1)
    expect(summary.outcomes[1]).toMatchObject({ status: 'skipped', reason: 'unchanged' })
  })

  it('honors dirty-tab skips passed by the client', async () => {
    const path = write('keep.txt', 'foo')
    const summary = await invoke(['keep.txt'], [path])
    expect(summary.outcomes[0]).toMatchObject({ status: 'skipped', reason: 'dirty-editor-tab' })
    expect(readFileSync(path, 'utf8')).toBe('foo')
  })

  it('fails closed on invalid replace queries before writing anything', async () => {
    write('a.txt', 'foo')
    await expect(
      searchReplaceOnRelay({
        rootPath: root,
        query: '',
        replacement: 'x',
        options: {},
        targets: [{ relativePath: 'a.txt' }]
      })
    ).rejects.toThrow(/Empty query/)
    expect(readFileSync(join(root, 'a.txt'), 'utf8')).toBe('foo')
  })

  it('rejects targets escaping the root', async () => {
    write('../escape.txt', 'foo')
    const summary = await invoke(['../escape.txt'])
    expect(summary.outcomes[0]).toMatchObject({ status: 'failed', reason: 'outside-root' })
    expect(readFileSync(join(root, '..', 'escape.txt'), 'utf8')).toBe('foo')
    rmSync(join(root, '..', 'escape.txt'))
  })

  it('rewrites through absolute roots untouched by expandTilde', async () => {
    write('abs.txt', 'foo foo')
    const summary = await invoke(['abs.txt'])
    expect(summary.writtenOccurrences).toBe(2)
  })
})
