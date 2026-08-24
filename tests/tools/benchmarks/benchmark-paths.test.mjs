import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { resolveBenchmarkRepoRoot } from './benchmark-paths.mjs'

describe('resolveBenchmarkRepoRoot', () => {
  it('resolves the application root from the nested benchmark directory', () => {
    const benchmarkDir = resolve('tests', 'tools', 'benchmarks')

    expect(resolveBenchmarkRepoRoot(benchmarkDir)).toBe(resolve('.'))
  })
})
