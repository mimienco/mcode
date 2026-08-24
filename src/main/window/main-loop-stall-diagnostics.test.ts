import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { installMainLoopStallDiagnostics } from './main-loop-stall-diagnostics'

describe('installMainLoopStallDiagnostics', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('stays silent while the loop keeps up', () => {
    let clock = 0
    const warn = vi.fn()
    installMainLoopStallDiagnostics({ now: () => clock, warn })
    for (let i = 0; i < 10; i += 1) {
      clock += 1_000
      vi.advanceTimersByTime(1_000)
    }
    expect(warn).not.toHaveBeenCalled()
  })

  it('reports the lower-bound block duration once the loop lags past the threshold', () => {
    let clock = 0
    const warn = vi.fn()
    installMainLoopStallDiagnostics({ now: () => clock, warn })
    clock += 1_000
    vi.advanceTimersByTime(1_000)
    expect(warn).not.toHaveBeenCalled()
    // Next tick fires 4_300ms late: the loop was blocked that long.
    clock += 4_300
    vi.advanceTimersByTime(1_000)
    expect(warn).toHaveBeenCalledTimes(1)
    expect(warn).toHaveBeenCalledWith('[main-stall] main event loop blocked for >=3300ms')
  })

  it('keeps warning for repeated stalls without dedupe', () => {
    let clock = 0
    const warn = vi.fn()
    installMainLoopStallDiagnostics({ now: () => clock, warn })
    clock += 6_000
    vi.advanceTimersByTime(1_000)
    clock += 6_000
    vi.advanceTimersByTime(1_000)
    expect(warn).toHaveBeenCalledTimes(2)
  })
})
