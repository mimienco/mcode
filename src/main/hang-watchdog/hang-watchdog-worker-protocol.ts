export const HANG_WATCHDOG_HEARTBEAT_INTERVAL_MS = 500
export const HANG_WATCHDOG_TIMEOUT_MS = 5_000
export const HANG_WATCHDOG_CHECK_INTERVAL_MS = 500

export type HangWatchdogWorkerData = {
  parentPid: number
  markerPath: string
  timeoutMs: number
  checkIntervalMs: number
}

export type MainToHangWatchdogWorkerMessage = { type: 'heartbeat' } | { type: 'shutdown' }

export type HangWatchdogWorkerToMainMessage = {
  type: 'hang-detected' | 'hang-resolved'
  unresponsiveMs: number
  observedAt: number
}
