import { describe, expect, it } from 'vitest'
import { checkManifest } from './check-renderer-chunk-budgets'

// PERF-03 stage B / T5: budget logic fails deterministically on oversize
// shells, oversized section chunks, and forbidden transitive imports.
function buildManifest(overrides = {}) {
  return {
    'assets/Settings-abc.js': {
      file: 'assets/Settings-abc.js',
      name: 'Settings',
      imports: ['assets/shared.js']
    },
    'assets/shared.js': { file: 'assets/shared.js', imports: [] },
    'assets/GeneralPane-xyz.js': {
      file: 'assets/GeneralPane-xyz.js',
      isDynamicEntry: true,
      imports: []
    },
    ...overrides
  }
}

const SIZES = {
  'assets/Settings-abc.js': 100_000,
  'assets/shared.js': 40_000,
  'assets/GeneralPane-xyz.js': 150_000,
  'assets/TerminalPane-forbidden.js': 500_000
}

const sizeOf = (file) => SIZES[file] ?? 0

describe('check-renderer-chunk-budgets', () => {
  it('passes a healthy manifest within budgets', () => {
    const result = checkManifest({
      manifest: buildManifest(),
      sizeOf,
      shellBudget: 250_000,
      sectionBudget: 200_000,
      forbiddenReachable: [/TerminalPane/]
    })
    expect(result.ok).toBe(true)
    expect(result.violations).toEqual([])
  })

  it('fails when the shell graph exceeds its byte budget', () => {
    const result = checkManifest({
      manifest: buildManifest(),
      sizeOf,
      shellBudget: 100_000,
      sectionBudget: 200_000,
      forbiddenReachable: []
    })
    expect(result.ok).toBe(false)
    expect(result.violations[0]).toContain('Settings shell graph is 140000 bytes')
  })

  it('fails when a forbidden TerminalPane chunk is statically reachable', () => {
    const manifest = buildManifest({
      'assets/TerminalPane-forbidden.js': {
        file: 'assets/TerminalPane-forbidden.js',
        imports: []
      }
    })
    const withEdge = {
      ...manifest,
      'assets/Settings-abc.js': {
        ...manifest['assets/Settings-abc.js'],
        imports: ['assets/shared.js', 'assets/TerminalPane-forbidden.js']
      }
    }
    const result = checkManifest({
      manifest: withEdge,
      sizeOf,
      shellBudget: 250_000,
      sectionBudget: 200_000,
      forbiddenReachable: [/TerminalPane/]
    })
    expect(result.ok).toBe(false)
    expect(result.violations.join('\n')).toContain('Forbidden chunk(s)')
  })

  it('fails when a dynamic section chunk exceeds the section budget', () => {
    const manifest = buildManifest({
      'assets/GeneralPane-oversize.js': {
        file: 'assets/GeneralPane-oversize.js',
        isDynamicEntry: true,
        imports: ['assets/shared.js']
      }
    })
    const sizes = { ...SIZES, 'assets/GeneralPane-oversize.js': 300_000 }
    const result = checkManifest({
      manifest,
      sizeOf: (file) => sizes[file] ?? 0,
      shellBudget: 250_000,
      sectionBudget: 200_000,
      forbiddenReachable: []
    })
    expect(result.ok).toBe(false)
    expect(result.violations.join('\n')).toContain('GeneralPane-oversize.js')
  })
})
