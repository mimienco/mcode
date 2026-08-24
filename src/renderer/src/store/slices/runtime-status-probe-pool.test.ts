import { describe, expect, it } from 'vitest'
import { RUNTIME_STATUS_PROBE_PARALLELISM, runWithPool } from './runtime-status-probe-pool'

// PERF-09: status probes for many servers run through a bounded pool instead
// of an unbounded allSettled wave.
describe('runtime status probe pool', () => {
  it('never exceeds its concurrency limit and runs every item', async () => {
    let active = 0
    let peak = 0
    const processed: number[] = []
    await runWithPool(
      Array.from({ length: 50 }, (_, i) => i),
      RUNTIME_STATUS_PROBE_PARALLELISM,
      async (item) => {
        active += 1
        peak = Math.max(peak, active)
        await new Promise((resolve) => setTimeout(resolve, 0))
        active -= 1
        processed.push(item)
      }
    )
    expect(peak).toBeLessThanOrEqual(RUNTIME_STATUS_PROBE_PARALLELISM)
    expect(processed).toHaveLength(50)
    expect(new Set(processed).size).toBe(50)
  })

  it('completes immediately for an empty queue', async () => {
    await expect(runWithPool([], 4, async () => {})).resolves.toBeUndefined()
  })
})
