import { recordDurableCrashBreadcrumb } from '../crash-reporting/durable-crash-breadcrumb'
import { appendStallRecord } from './main-stall-log'

type MainLoopStallDiagnosticsOptions = {
  /** Poll interval in ms. Drift is measured as the overshoot past this interval. */
  tickMs?: number
  /** Minimum drift in ms before a stall is reported. */
  stallThresholdMs?: number
  now?: () => number
  warn?: (message: string) => void
}

type StallTimer = { unref?: () => unknown }

// Why: production calls this once per main-window creation with no options; the
// first no-options install wins so stacked windows never stack heartbeats.
// Tests pass explicit options and always get a fresh timer.
let productionSingleton: StallTimer | null = null

/**
 * Why: Windows marks the whole app "Not responding" when the MAIN process
 * message pump stalls (~5s), and CPU-bound main-thread work (e.g. terminal
 * buffer snapshot serialization) stalls exactly here — invisibly, unless
 * something measures it. A 1s heartbeat turns any such block into a logged
 * lower-bound duration, correlatable with the `[terminal-snapshot]` warnings.
 */
export function installMainLoopStallDiagnostics(
  options: MainLoopStallDiagnosticsOptions = {}
): StallTimer {
  if (!options.now && !options.warn && productionSingleton) {
    return productionSingleton
  }
  const tickMs = options.tickMs ?? 1_000
  const stallThresholdMs = options.stallThresholdMs ?? 1_000
  const now = options.now ?? (() => Date.now())
  const warn = options.warn ?? ((message: string) => console.warn(message))
  let lastTickAt = now()
  const timer = setInterval(() => {
    const current = now()
    const drift = current - lastTickAt - tickMs
    lastTickAt = current
    if (drift >= stallThresholdMs) {
      warn(`[main-stall] main event loop blocked for >=${drift}ms`)
      appendStallRecord('main_loop_stall', { driftMs: drift })
      try {
        recordDurableCrashBreadcrumb('main_loop_stall', { driftMs: drift })
      } catch {
        // Diagnostics must never be the thing that takes the app down.
      }
    }
  }, tickMs)
  // Why: a diagnostics heartbeat must never keep the process alive on quit.
  timer.unref?.()
  if (!options.now && !options.warn) {
    productionSingleton = timer
  }
  return timer
}
