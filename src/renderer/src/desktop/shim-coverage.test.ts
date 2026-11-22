import { describe, expect, it } from 'vitest'
import { ALL_WINDOW_API_NAMESPACES, SHIMMED_NAMESPACES, computeShimCoverage } from './shim-coverage'

describe('shim-coverage', () => {
  it('tracks exactly the 78 namespaces the renderer touches', () => {
    expect(ALL_WINDOW_API_NAMESPACES).toHaveLength(78)
    expect(ALL_WINDOW_API_NAMESPACES).toContain('e2e')
    expect(ALL_WINDOW_API_NAMESPACES).toContain('speech')
    expect(ALL_WINDOW_API_NAMESPACES).toContain('agentHooks')
  })

  it('serves exactly ten namespaces natively, including agentStatus and agentHooks', () => {
    expect(SHIMMED_NAMESPACES).toHaveLength(10)
    expect(SHIMMED_NAMESPACES).toContain('session')
    expect(SHIMMED_NAMESPACES).toContain('agentStatus')
    expect(SHIMMED_NAMESPACES).toContain('agentHooks')
  })

  it('keeps both lists sorted and duplicate-free', () => {
    for (const list of [ALL_WINDOW_API_NAMESPACES, SHIMMED_NAMESPACES]) {
      const sorted = [...list].sort()
      expect([...list]).toEqual(sorted)
      expect(new Set(list).size).toBe(list.length)
    }
  })

  it('classifies the shimmed namespaces as covered', () => {
    const coverage = computeShimCoverage()
    expect(coverage.covered).toEqual([...SHIMMED_NAMESPACES])
    expect(coverage.covered).toContain('speech')
    expect(coverage.pending).not.toContain('speech')
    expect(coverage.pending).not.toContain('ui')
    expect(coverage.pending).not.toContain('notifications')
    expect(coverage.pending).not.toContain('fs')
    expect(coverage.pending).not.toContain('shell')
    expect(coverage.pending).not.toContain('updater')
    expect(coverage.pending).not.toContain('session')
    expect(coverage.pending).not.toContain('agentStatus')
    expect(coverage.pending).not.toContain('agentHooks')
    expect(coverage.covered).toContain('agentStatus')
    expect(coverage.covered).toContain('agentHooks')
  })

  it('leaves every other namespace pending', () => {
    const coverage = computeShimCoverage()
    expect(coverage.pending).toHaveLength(
      ALL_WINDOW_API_NAMESPACES.length - SHIMMED_NAMESPACES.length
    )
    expect(coverage.pending).toContain('pty')
    expect(coverage.pending).toContain('settings')
  })

  it('flags shimmed namespaces missing from the renderer extraction as unexpected', () => {
    const coverage = computeShimCoverage(['speech', 'ui'], ['notifications', 'speech', 'ui'])
    expect(coverage.unexpected).toEqual(['notifications'])
  })

  it('accepts fully clean ledgers', () => {
    const coverage = computeShimCoverage(['speech', 'ui'], ['speech', 'ui'])
    expect(coverage.unexpected).toEqual([])
    expect(coverage.covered).toEqual(['speech', 'ui'])
    expect(coverage.pending).toEqual([])
  })

  it('rejects unsorted ledger input', () => {
    expect(() => computeShimCoverage(['b', 'a'], [])).toThrow('namespace lists must be sorted')
    expect(() => computeShimCoverage(['a'], ['b', 'a'])).toThrow('namespace lists must be sorted')
  })
})
