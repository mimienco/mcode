import { describe, expect, it, vi } from 'vitest'
import { recoverLegacyWorkerTerminalsForRendererStartup } from './legacy-worker-renderer-recovery'

// PERF-02: renderer startup recovery must cost zero reconciles when there are
// no legacy candidates, coalesce concurrent calls onto one pass, and skip a
// post-reconnect pass over an unchanged candidate landscape.
function baseOptions(
  overrides: Partial<Parameters<typeof recoverLegacyWorkerTerminalsForRendererStartup>[0]> = {}
) {
  return {
    firstWindowStartupServicesReady: Promise.resolve(),
    managedWslCliStartupBarrierReady: Promise.resolve(),
    localPtyProviderStartupReady: Promise.resolve(),
    reconcile: vi.fn().mockResolvedValue(undefined),
    onDeferredRecoveryError: vi.fn(),
    ...overrides
  }
}

describe('legacy worker recovery fast path and coalescing', () => {
  it('makes zero reconcile calls when the inventory reports no candidates', async () => {
    const options = baseOptions({
      hasLegacyCandidates: () => false
    })

    const result = await recoverLegacyWorkerTerminalsForRendererStartup(options)

    expect(result).toMatchObject({ skipped: 'no_candidates', reconcileCount: 0 })
    expect(options.reconcile).not.toHaveBeenCalled()
    expect(options.onDeferredRecoveryError).not.toHaveBeenCalled()
  })

  it('joins concurrent calls into the running pass', async () => {
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const reconcile = vi.fn(() => gate.then(() => undefined))
    const options = baseOptions({ reconcile })
    const first = recoverLegacyWorkerTerminalsForRendererStartup(options)
    const second = recoverLegacyWorkerTerminalsForRendererStartup(options)

    // Give the second call a chance to (wrongly) start its own lanes.
    await new Promise((resolve) => setTimeout(resolve, 20))
    release()

    const [firstResult, secondResult] = await Promise.all([first, second])
    const totalCalls = reconcile.mock.calls.length
    expect(totalCalls).toBe(2) // immediate + deferred lane of ONE pass
    expect(firstResult.skipped).toBeNull()
    expect(secondResult.skipped).toBe('joined_in_flight')
  })

  it('skips a post-pass over an unchanged landscape but re-runs when it changes', async () => {
    let hasCandidates = true
    const reconcile = vi.fn().mockResolvedValue(undefined)
    const options = baseOptions({
      reconcile,
      hasLegacyCandidates: () => hasCandidates
    })

    const first = await recoverLegacyWorkerTerminalsForRendererStartup(options)
    expect(first.skipped).toBeNull()
    const callsAfterFirstPass = reconcile.mock.calls.length

    // Why: wait for the deferred provider-ready lane to settle so this is a
    // genuine post-pass call rather than an in-flight join.
    await new Promise((resolve) => setTimeout(resolve, 20))

    // Post-reconnect call over the same landscape: skipped.
    const second = await recoverLegacyWorkerTerminalsForRendererStartup(options)
    expect(second.skipped).toBe('landscape_unchanged')
    expect(second.reconcileCount).toBe(0)
    expect(reconcile.mock.calls.length).toBe(callsAfterFirstPass)

    // Landscape change (candidate count differs) triggers a fresh pass.
    hasCandidates = false
    const third = await recoverLegacyWorkerTerminalsForRendererStartup(options)
    expect(third.skipped).toBe('no_candidates')
  })
})
