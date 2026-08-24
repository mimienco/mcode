export type LegacyWorkerRendererRecoveryOptions = {
  firstWindowStartupServicesReady: Promise<void>
  managedWslCliStartupBarrierReady: Promise<void>
  localPtyProviderStartupReady: Promise<void>
  reconcile: () => Promise<unknown> | undefined
  onDeferredRecoveryError: (error: unknown) => void
  /** Why: PERF-02 — cheap candidate inventory probe; absent means "unknown",
   * which keeps the legacy always-attempt behavior. */
  hasLegacyCandidates?: () => boolean | Promise<boolean>
}

export type LegacyWorkerRendererRecoveryResult = {
  /** Why the call did (or did not) reconcile; null means it reconciled. */
  skipped: 'no_candidates' | 'landscape_unchanged' | 'joined_in_flight' | null
  candidateCount: number | null
  reconcileCount: number
  waitMs: number
}

// Why: the PTY controller attaches shortly after provider startup, so the first
// reconcile can race it and throw terminal_liveness_unavailable. Retry briefly
// instead of logging the same startup-race warning on every launch; if liveness
// is still unavailable after the window, something is genuinely wrong and the
// warning fires once through the normal path.
const LIVENESS_RETRY_DELAY_MS = 500
const LIVENESS_RETRY_WINDOW_MS = 2_500

function isTerminalLivenessUnavailable(error: unknown): boolean {
  return error instanceof Error && error.message.includes('terminal_liveness_unavailable')
}

type RecoveryPassContext = {
  reconcileCount: number
  waitMs: number
  hadLivenessFailure: boolean
}

async function reconcileWithLivenessRetry(
  options: LegacyWorkerRendererRecoveryOptions,
  context: RecoveryPassContext,
  deadline: number
): Promise<void> {
  for (;;) {
    try {
      await options.reconcile()
      return
    } catch (error) {
      if (!isTerminalLivenessUnavailable(error)) {
        options.onDeferredRecoveryError(error)
        return
      }
      context.hadLivenessFailure = true
      // Why: PERF-02 — monotone deadline instead of an unbounded attempt chain.
      const now = Date.now()
      if (now >= deadline) {
        options.onDeferredRecoveryError(error)
        return
      }
      const waitMs = Math.min(LIVENESS_RETRY_DELAY_MS, deadline - now)
      context.waitMs += waitMs
      await new Promise((resolve) => setTimeout(resolve, waitMs))
    }
  }
}

// Why: PERF-02 — module-scoped pass state so the renderer's pre- and
// post-reconnect IPC calls coalesce onto one recovery pass instead of stacking
// two full lanes, and a post-pass over an unchanged candidate landscape skips.
type ActivePass = {
  /** Resolves as soon as the immediate lane finished (renderer-facing timing). */
  resultPromise: Promise<LegacyWorkerRendererRecoveryResult>
  /** Resolves when even the deferred provider-ready lane has settled. */
  settled: Promise<void>
}
let activePass: ActivePass | null = null
let lastCompletedCandidateCount: number | null = null
let lastPassHadLivenessFailure = false

async function resolveCandidateCount(
  options: LegacyWorkerRendererRecoveryOptions
): Promise<number | null> {
  if (!options.hasLegacyCandidates) {
    return null
  }
  try {
    return (await options.hasLegacyCandidates()) ? 1 : 0
  } catch {
    // Why: an unavailable inventory is "unclear", never "dead" — attempt recovery.
    return null
  }
}

export async function recoverLegacyWorkerTerminalsForRendererStartup(
  options: LegacyWorkerRendererRecoveryOptions
): Promise<LegacyWorkerRendererRecoveryResult> {
  const candidateCount = await resolveCandidateCount(options)

  // Why: PERF-02 — zero legacy candidates must cost zero reconcile calls and
  // zero retry windows, even before any barrier resolves.
  if (candidateCount === 0) {
    lastCompletedCandidateCount = 0
    lastPassHadLivenessFailure = false
    return { skipped: 'no_candidates', candidateCount: 0, reconcileCount: 0, waitMs: 0 }
  }

  if (activePass) {
    await activePass.settled
    return { skipped: 'joined_in_flight', candidateCount, reconcileCount: 0, waitMs: 0 }
  }

  // Why: PERF-02 — after a clean pass, a second call over an unchanged
  // candidate landscape would only repeat identical reconciles.
  if (
    !lastPassHadLivenessFailure &&
    lastCompletedCandidateCount !== null &&
    candidateCount !== null &&
    candidateCount === lastCompletedCandidateCount
  ) {
    return {
      skipped: 'landscape_unchanged',
      candidateCount,
      reconcileCount: 0,
      waitMs: 0
    }
  }

  const context: RecoveryPassContext = {
    reconcileCount: 0,
    waitMs: 0,
    hadLivenessFailure: false
  }
  const deadline = Date.now() + LIVENESS_RETRY_WINDOW_MS

  let resolveImmediate!: (result: LegacyWorkerRendererRecoveryResult) => void
  let resolveSettled!: () => void
  const resultPromise = new Promise<LegacyWorkerRendererRecoveryResult>((resolve) => {
    resolveImmediate = resolve
  })
  const settled = new Promise<void>((resolve) => {
    resolveSettled = resolve
  })
  activePass = { resultPromise, settled }

  const finishPass = (): void => {
    lastCompletedCandidateCount = candidateCount
    lastPassHadLivenessFailure = context.hadLivenessFailure
    activePass = null
    resolveSettled()
  }

  void (async () => {
    const providerStartupResult = options.localPtyProviderStartupReady.then(
      () => ({ ok: true as const }),
      (error: unknown) => ({ ok: false as const, error })
    )
    try {
      await Promise.all([
        options.firstWindowStartupServicesReady,
        options.managedWslCliStartupBarrierReady
      ])
      context.reconcileCount += 1
      await reconcileWithLivenessRetry(options, context, deadline)
    } finally {
      resolveImmediate({
        skipped: null,
        candidateCount,
        reconcileCount: context.reconcileCount,
        waitMs: context.waitMs
      })
    }
    void providerStartupResult
      .then(async (result) => {
        if (!result.ok) {
          throw result.error
        }
        context.reconcileCount += 1
        await reconcileWithLivenessRetry(options, context, deadline)
      })
      .catch(options.onDeferredRecoveryError)
      .finally(finishPass)
  })()

  return resultPromise
}
