import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import test from 'node:test'

const guardScript = resolve(import.meta.dirname, 'verify-original-integrity.mjs')

function git(cwd, args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8' })
}

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), 'mcode-source-integrity-'))
  const source = join(root, 'source')
  const snapshot = join(root, 'snapshot')
  mkdirSync(source)
  mkdirSync(snapshot)
  git(source, ['init', '--quiet'])
  git(source, ['config', 'user.email', 'source-integrity@example.com'])
  git(source, ['config', 'user.name', 'Source Integrity'])
  writeFileSync(join(source, 'tracked.txt'), 'captured\n')
  git(source, ['add', 'tracked.txt'])
  git(source, ['commit', '--quiet', '-m', 'captured source'])
  git(source, ['switch', '-c', 'captured-branch'])
  writeFileSync(join(source, 'untracked.txt'), 'preserve me\n')

  writeFileSync(join(snapshot, 'HEAD.txt'), git(source, ['rev-parse', 'HEAD']))
  writeFileSync(join(snapshot, 'branch.txt'), git(source, ['branch', '--show-current']))
  writeFileSync(join(snapshot, 'diff-binary.patch'), git(source, ['diff', '--binary']))
  writeFileSync(
    join(snapshot, 'diff-cached-binary.patch'),
    git(source, ['diff', '--cached', '--binary'])
  )
  writeFileSync(
    join(snapshot, 'status-full.txt'),
    git(source, ['status', '--porcelain=v1', '-uall'])
  )
  const hash = (value) => createHash('sha256').update(value).digest('hex')
  writeFileSync(
    join(snapshot, 'manifest.sha256'),
    `${hash('captured\n')} *tracked.txt\n${hash('preserve me\n')} *untracked.txt\n`
  )
  return { root, source, snapshot }
}

test('reports a captured source repository as unchanged', () => {
  const fixture = createFixture()
  try {
    const result = spawnSync(
      process.execPath,
      [guardScript, '--source', fixture.source, '--snapshot', fixture.snapshot, '--json'],
      { encoding: 'utf8' }
    )

    assert.equal(result.status, 0, result.stderr)
    const report = JSON.parse(result.stdout)
    assert.equal(report.unchanged, true)
    assert.deepEqual(
      report.checks.map(({ name, unchanged }) => ({ name, unchanged })),
      [
        { name: 'HEAD', unchanged: true },
        { name: 'branch', unchanged: true },
        { name: 'unstaged diff', unchanged: true },
        { name: 'staged diff', unchanged: true },
        { name: 'working tree status', unchanged: true },
        { name: 'source manifest', unchanged: true }
      ]
    )
  } finally {
    rmSync(fixture.root, { recursive: true, force: true })
  }
})

test('reports changed checks with an explicit boolean false', () => {
  const fixture = createFixture()
  try {
    git(fixture.source, ['switch', '-c', 'different-branch'])
    const result = spawnSync(
      process.execPath,
      [guardScript, '--source', fixture.source, '--snapshot', fixture.snapshot, '--json'],
      { encoding: 'utf8' }
    )

    assert.equal(result.status, 1, result.stderr)
    const report = JSON.parse(result.stdout)
    const branch = report.checks.find((check) => check.name === 'branch')
    assert.equal(branch.unchanged, false)
  } finally {
    rmSync(fixture.root, { recursive: true, force: true })
  }
})
