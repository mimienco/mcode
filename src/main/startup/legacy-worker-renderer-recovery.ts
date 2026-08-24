type LegacyWorkerRendererRecoveryOptions = {
  firstWindowStartupServicesReady: Promise<void>
  managedWslCliStartupBarrierReady: Promise<void>
  localPtyProviderStartupReady: Promise<void>
  reconcile: () => Promise<unknown> | undefined
  onDeferredRecoveryError: (error: unknown) => void
}

// Why: the PTY controller attaches shortly after provider startup, so the first
// reconcile can race it and throw terminal_liveness_unavailable. Retry briefly
// instead of logging the same startup-race warning on every launch; if liveness
// is still unavailable after the window, something is genuinely wrong and the
// warning fires once through the normal path.
const LIVENESS_RETRY_ATTEMPTS = 5
const LIVENESS_RETRY_DELAY_MS = 500

function isTerminalLivenessUnavailable(error: unknown): boolean {
  return error instanceof Error && error.message.includes('terminal_liveness_unavailable')
}

async function reconcileWithLivenessRetry(options: LegacyWorkerRendererRecoveryOptions): Promise<void> {
  let attemptsLeft = LIVENESS_RETRY_ATTEMPTS
  for (;;) {
    try {
      await options.reconcile()
      return
    } catch (error) {
      if (!isTerminalLivenessUnavailable(error) || attemptsLeft === 0) {
        options.onDeferredRecoveryError(error)
        return
      }
      attemptsLeft -= 1
      await new Promise((resolve) => setTimeout(resolve, LIVENESS_RETRY_DELAY_MS))
    }
  }
}

export async function recoverLegacyWorkerTerminalsForRendererStartup(
  options: LegacyWorkerRendererRecoveryOptions
): Promise<void> {
  const providerStartupResult = options.localPtyProviderStartupReady.then(
    () => ({ ok: true as const }),
    (error: unknown) => ({ ok: false as const, error })
  )
  await Promise.all([
    options.firstWindowStartupServicesReady,
    options.managedWslCliStartupBarrierReady
  ])
  void providerStartupResult
    .then(async (result) => {
      if (!result.ok) {
        throw result.error
      }
      await reconcileWithLivenessRetry(options)
    })
    .catch(options.onDeferredRecoveryError)
  await reconcileWithLivenessRetry(options)
}
