import { describe, expect, it, vi } from 'vitest'
import { recoverLegacyWorkerTerminalsForRendererStartup } from './legacy-worker-renderer-recovery'

describe('legacy worker renderer recovery', () => {
  it('hydrates after bounded barriers while provider startup remains pending', async () => {
    let resolveFirstWindow!: () => void
    let resolveWslBarrier!: () => void
    let resolveProvider!: () => void
    const firstWindowReady = new Promise<void>((resolve) => {
      resolveFirstWindow = resolve
    })
    const wslBarrierReady = new Promise<void>((resolve) => {
      resolveWslBarrier = resolve
    })
    const providerReady = new Promise<void>((resolve) => {
      resolveProvider = resolve
    })
    const reconcile = vi.fn().mockResolvedValue(undefined)
    const startup = recoverLegacyWorkerTerminalsForRendererStartup({
      firstWindowStartupServicesReady: firstWindowReady,
      managedWslCliStartupBarrierReady: wslBarrierReady,
      localPtyProviderStartupReady: providerReady,
      reconcile,
      onDeferredRecoveryError: vi.fn()
    })

    resolveFirstWindow()
    await Promise.resolve()
    expect(reconcile).not.toHaveBeenCalled()

    resolveWslBarrier()
    await startup
    expect(reconcile).toHaveBeenCalledTimes(1)

    resolveProvider()
    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(2))
  })

  it('retries after initial recovery when the provider is already ready', async () => {
    const reconcile = vi.fn().mockResolvedValue(undefined)

    await recoverLegacyWorkerTerminalsForRendererStartup({
      firstWindowStartupServicesReady: Promise.resolve(),
      managedWslCliStartupBarrierReady: Promise.resolve(),
      localPtyProviderStartupReady: Promise.resolve(),
      reconcile,
      onDeferredRecoveryError: vi.fn()
    })

    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(2))
  })

  it('contains provider startup rejection after initial recovery', async () => {
    const providerError = new Error('provider failed')
    const reconcile = vi.fn().mockResolvedValue(undefined)
    let reportError!: (error: unknown) => void
    const reportedError = new Promise<unknown>((resolve) => {
      reportError = resolve
    })

    await recoverLegacyWorkerTerminalsForRendererStartup({
      firstWindowStartupServicesReady: Promise.resolve(),
      managedWslCliStartupBarrierReady: Promise.resolve(),
      localPtyProviderStartupReady: Promise.reject(providerError),
      reconcile,
      onDeferredRecoveryError: reportError
    })

    await expect(reportedError).resolves.toBe(providerError)
    expect(reconcile).toHaveBeenCalledTimes(1)
  })

  it('contains deferred recovery rejection', async () => {
    const recoveryError = new Error('recovery failed')
    const reconcile = vi.fn().mockResolvedValueOnce(undefined).mockRejectedValueOnce(recoveryError)
    let reportError!: (error: unknown) => void
    const reportedError = new Promise<unknown>((resolve) => {
      reportError = resolve
    })

    await recoverLegacyWorkerTerminalsForRendererStartup({
      firstWindowStartupServicesReady: Promise.resolve(),
      managedWslCliStartupBarrierReady: Promise.resolve(),
      localPtyProviderStartupReady: Promise.resolve(),
      reconcile,
      onDeferredRecoveryError: reportError
    })

    await expect(reportedError).resolves.toBe(recoveryError)
    expect(reconcile).toHaveBeenCalledTimes(2)
  })

  it('contains initial recovery rejection and still retries when the provider becomes ready', async () => {
    const initialError = new Error('initial recovery failed')
    let resolveProvider!: () => void
    const providerReady = new Promise<void>((resolve) => {
      resolveProvider = resolve
    })
    const reconcile = vi.fn().mockRejectedValueOnce(initialError).mockResolvedValueOnce(undefined)
    const onDeferredRecoveryError = vi.fn()

    await expect(
      recoverLegacyWorkerTerminalsForRendererStartup({
        firstWindowStartupServicesReady: Promise.resolve(),
        managedWslCliStartupBarrierReady: Promise.resolve(),
        localPtyProviderStartupReady: providerReady,
        reconcile,
        onDeferredRecoveryError
      })
    ).resolves.toMatchObject({ skipped: null })

    expect(onDeferredRecoveryError).toHaveBeenCalledWith(initialError)
    expect(reconcile).toHaveBeenCalledTimes(1)
    resolveProvider()
    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(2))
  })

  it('retries a terminal_liveness_unavailable startup race without reporting an error', async () => {
    const livenessError = new Error('terminal_liveness_unavailable')
    const reconcile = vi.fn().mockRejectedValueOnce(livenessError).mockResolvedValueOnce(undefined)
    const onDeferredRecoveryError = vi.fn()

    await recoverLegacyWorkerTerminalsForRendererStartup({
      firstWindowStartupServicesReady: Promise.resolve(),
      managedWslCliStartupBarrierReady: Promise.resolve(),
      localPtyProviderStartupReady: Promise.resolve(),
      reconcile,
      onDeferredRecoveryError
    })

    // Why 3: the immediate lane retries once after the liveness race clears (2 calls)
    // and the deferred provider-ready lane reconciles once more.
    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(3))
    expect(onDeferredRecoveryError).not.toHaveBeenCalled()
  })

  it('reports through the normal path once liveness retries are exhausted', async () => {
    // Why: fake timers — real 500ms backoffs would stall the suite.
    vi.useFakeTimers()
    try {
      const livenessError = new Error('terminal_liveness_unavailable')
      const reconcile = vi.fn().mockRejectedValue(livenessError)
      const onDeferredRecoveryError = vi.fn()

      const startup = recoverLegacyWorkerTerminalsForRendererStartup({
        firstWindowStartupServicesReady: Promise.resolve(),
        managedWslCliStartupBarrierReady: Promise.resolve(),
        localPtyProviderStartupReady: Promise.resolve(),
        reconcile,
        onDeferredRecoveryError
      })
      await vi.runAllTimersAsync()
      await startup

      // Why: PERF-02 — both lanes share one monotone deadline, so the total
      // attempt count is bounded instead of a fixed additive chain.
      const calls = reconcile.mock.calls.length
      expect(calls).toBeGreaterThan(0)
      expect(calls).toBeLessThanOrEqual(12)
      expect(onDeferredRecoveryError).toHaveBeenCalledWith(livenessError)
    } finally {
      vi.useRealTimers()
    }
  })
})
