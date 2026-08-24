import { spawn, type ChildProcess } from 'node:child_process'
import { createHash } from 'node:crypto'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { normalizeRuntimePathForComparison } from '../../shared/cross-platform-path'
import { parseWslUncPath } from '../../shared/wsl-paths'
import { withManagedHookInstallLock } from '../agent-hooks/managed-hook-install-lock'
import { readManagedHookHostIdentity } from '../agent-hooks/managed-hook-owner-identity'
import { buildWslCodexAppServerArgs } from '../codex-accounts/wsl-codex-command'
import { resolveCodexCommand } from '../codex-cli/command'
import { terminateCodexProbeChild } from '../rate-limits/codex-probe-termination'
import { getSpawnArgsForWindows } from '../win32-utils'
import { getMCodeUserDataPath } from './codex-home-paths'
import { quarantineCodexStateDb } from './codex-state-db-repair'
import {
  BACKFILL_PENDING_MIN_SESSION_FILES,
  countCodexSessionFilesUpTo,
  isCodexStateDbBackfillPending,
  readCodexStateDbBackfillStatus,
  type CodexStateDbBackfillStatus
} from './codex-state-db'

const RECOVERY_POLL_INTERVAL_MS = 5_000
const RECOVERY_RETRY_DELAY_MS = 2_000
const RECOVERY_MAX_COORDINATOR_FAILURES = 5
const RECOVERY_MAX_SPAWNS = 5
const RECOVERY_MAX_TOTAL_MS = 60 * 60_000
const RECOVERY_OWNER_CHECK_TIMEOUT_MS = 1_000
const RECOVERY_MAX_STATE_DB_REBUILDS = 1
// Keep the recovery claimant independent of approval-policy spelling changes;
// read-only mode is the only policy this maintenance app-server needs.
const RECOVERY_CODEX_ARGS = ['-s', 'read-only', 'app-server'] as const
const RECOVERY_SUPERVISOR_RETRY_DELAY_MS = 30_000
const RECOVERY_STATE_DB_REPAIR_COOLDOWN_MS = 10 * 60_000

export type CodexStateDbBackfillRecoverySummary = {
  outcome: 'completed' | 'already-complete' | 'not-needed' | 'unreadable' | 'stopped' | 'gave-up'
  spawnCount: number
}

type RecoveryDependencies = {
  spawnProcess: typeof spawn
  resolveCommand: () => string
  readStatus: (codexHomePath: string) => CodexStateDbBackfillStatus
  countSessions: (sessionsRoot: string, limit: number) => number
  now: () => number
  sleep: (ms: number, signal: AbortSignal) => Promise<void>
  terminate: (child: ChildProcess) => Promise<void>
  quarantineStateDb: (codexHomePath: string, stateDbPath: string) => Promise<string | null>
}

const defaultDependencies: RecoveryDependencies = {
  spawnProcess: spawn,
  resolveCommand: resolveCodexCommand,
  readStatus: readCodexStateDbBackfillStatus,
  countSessions: countCodexSessionFilesUpTo,
  now: Date.now,
  sleep: async (ms, signal) => await delay(ms, undefined, { signal }),
  terminate: async (child) => await terminateCodexProbeChild(child),
  quarantineStateDb: quarantineCodexStateDb
}

function finish(
  outcome: CodexStateDbBackfillRecoverySummary['outcome'],
  spawnCount: number
): CodexStateDbBackfillRecoverySummary {
  return { outcome, spawnCount }
}

function initialRecoveryDecision(
  codexHomePath: string,
  dependencies: RecoveryDependencies
): CodexStateDbBackfillRecoverySummary['outcome'] | null {
  const status = dependencies.readStatus(codexHomePath)
  if (status.kind === 'complete') {
    return 'already-complete'
  }
  if (status.kind === 'unreadable') {
    return 'unreadable'
  }
  if (
    (status.kind === 'missing' || status.kind === 'not-tracked') &&
    dependencies.countSessions(
      join(codexHomePath, 'sessions'),
      BACKFILL_PENDING_MIN_SESSION_FILES
    ) < BACKFILL_PENDING_MIN_SESSION_FILES
  ) {
    return 'not-needed'
  }
  return null
}

function spawnRecoveryProcess(
  codexHomePath: string,
  dependencies: RecoveryDependencies
): ChildProcess {
  const wslHome = process.platform === 'win32' ? parseWslUncPath(codexHomePath) : null
  if (wslHome) {
    return dependencies.spawnProcess(
      'wsl.exe',
      buildWslCodexAppServerArgs(wslHome.distro, wslHome.linuxPath),
      {
        stdio: ['pipe', 'ignore', 'ignore'],
        windowsHide: true,
        env: process.env
      }
    )
  }
  const command = dependencies.resolveCommand()
  const { spawnCmd, spawnArgs } = getSpawnArgsForWindows(command, [...RECOVERY_CODEX_ARGS])
  return dependencies.spawnProcess(spawnCmd, spawnArgs, {
    cwd: codexHomePath,
    stdio: ['pipe', 'ignore', 'ignore'],
    windowsHide: true,
    env: { ...process.env, CODEX_HOME: codexHomePath }
  })
}

/** Keeps a sanctioned app-server claimant alive until Codex completes its own backfill. */
export async function runCodexStateDbBackfillRecovery(
  codexHomePath: string,
  signal: AbortSignal,
  dependenciesOverride: Partial<RecoveryDependencies> = {}
): Promise<CodexStateDbBackfillRecoverySummary> {
  const dependencies = { ...defaultDependencies, ...dependenciesOverride }
  const deadline = dependencies.now() + RECOVERY_MAX_TOTAL_MS
  let rebuildCount = 0
  let spawnCount = 0
  while (!signal.aborted && dependencies.now() < deadline) {
    const initialOutcome = initialRecoveryDecision(codexHomePath, dependencies)
    if (initialOutcome) {
      if (initialOutcome === 'unreadable') {
        const status = dependencies.readStatus(codexHomePath)
        if (await tryQuarantineStateDb(codexHomePath, status, rebuildCount, dependencies)) {
          rebuildCount += 1
          continue
        }
      }
      return finish(initialOutcome, spawnCount)
    }

    let phaseSpawnCount = 0
    let phaseStatus: CodexStateDbBackfillStatus | undefined
    while (
      !signal.aborted &&
      dependencies.now() < deadline &&
      phaseSpawnCount < RECOVERY_MAX_SPAWNS
    ) {
      const child = spawnRecoveryProcess(codexHomePath, dependencies)
      phaseSpawnCount += 1
      spawnCount += 1
      let childDown = false
      child.once('error', () => {
        childDown = true
      })
      child.once('exit', () => {
        childDown = true
      })

      try {
        while (!childDown && !signal.aborted && dependencies.now() < deadline) {
          await dependencies.sleep(RECOVERY_POLL_INTERVAL_MS, signal)
          phaseStatus = dependencies.readStatus(codexHomePath)
          if (phaseStatus.kind === 'complete') {
            await dependencies.terminate(child)
            return finish('completed', spawnCount)
          }
          if (phaseStatus.kind === 'unreadable') {
            await dependencies.terminate(child)
            break
          }
        }
      } catch (error) {
        if (!signal.aborted) {
          await dependencies.terminate(child)
          throw error
        }
      }

      if (signal.aborted) {
        await dependencies.terminate(child)
        return finish('stopped', spawnCount)
      }
      if (phaseStatus?.kind === 'unreadable') {
        break
      }
      if (!childDown) {
        await dependencies.terminate(child)
        break
      }
      if (phaseSpawnCount >= RECOVERY_MAX_SPAWNS) {
        break
      }
      try {
        await dependencies.sleep(RECOVERY_RETRY_DELAY_MS, signal)
      } catch {
        return finish('stopped', spawnCount)
      }
    }

    if (signal.aborted) {
      return finish('stopped', spawnCount)
    }

    const finalStatus = phaseStatus ?? dependencies.readStatus(codexHomePath)
    if (await tryQuarantineStateDb(codexHomePath, finalStatus, rebuildCount, dependencies)) {
      rebuildCount += 1
      continue
    }
    return finish(finalStatus.kind === 'unreadable' ? 'unreadable' : 'gave-up', spawnCount)
  }
  return finish(signal.aborted ? 'stopped' : 'gave-up', spawnCount)
}

async function tryQuarantineStateDb(
  codexHomePath: string,
  status: CodexStateDbBackfillStatus,
  rebuildCount: number,
  dependencies: RecoveryDependencies
): Promise<boolean> {
  if (
    rebuildCount >= RECOVERY_MAX_STATE_DB_REBUILDS ||
    (status.kind !== 'incomplete' && status.kind !== 'unreadable')
  ) {
    return false
  }
  const key = normalizeRuntimePathForComparison(codexHomePath)
  const lastRepairAt = stateDbRepairTimes.get(key)
  if (
    lastRepairAt !== undefined &&
    dependencies.now() - lastRepairAt < RECOVERY_STATE_DB_REPAIR_COOLDOWN_MS
  ) {
    return false
  }
  try {
    const recoveryDirectory = await dependencies.quarantineStateDb(
      codexHomePath,
      status.stateDbPath
    )
    if (!recoveryDirectory) {
      return false
    }
    stateDbRepairTimes.set(key, dependencies.now())
    console.warn(
      `[codex-state-db-backfill] quarantined incomplete state DB at ${status.stateDbPath}; rebuilding from session history (${recoveryDirectory})`
    )
    return true
  } catch (error) {
    console.warn('[codex-state-db-backfill] state DB quarantine failed:', error)
    return false
  }
}

export function resolveCodexBackfillSupervisorLockRoot(codexHomePath: string): string {
  const homeKey = normalizeRuntimePathForComparison(codexHomePath)
  const digest = createHash('sha256').update(homeKey).digest('hex')
  return join(getMCodeUserDataPath(), 'codex-state-db-backfill-locks', digest)
}

function scopeRecoveryHostIdentity(hostIdentity: string, codexHomePath: string): string {
  const wslHome = process.platform === 'win32' ? parseWslUncPath(codexHomePath) : null
  return wslHome ? `${hostIdentity}:wsl:${wslHome.distro.toLowerCase()}` : hostIdentity
}

export async function withCodexBackfillSupervisorLock<T>(
  codexHomePath: string,
  signal: AbortSignal | undefined,
  run: () => Promise<T>
): Promise<T> {
  const hostIdentity = scopeRecoveryHostIdentity(await readManagedHookHostIdentity(), codexHomePath)
  // Reuse the crash-safe hard-link claim protocol; its storage root is Codex-specific.
  return await withManagedHookInstallLock(
    resolveCodexBackfillSupervisorLockRoot(codexHomePath),
    signal,
    run,
    hostIdentity,
    { waitTimeoutMs: RECOVERY_OWNER_CHECK_TIMEOUT_MS }
  )
}

type ActiveRecovery = {
  controller: AbortController
  ready: Promise<void>
  task: Promise<CodexStateDbBackfillRecoverySummary | null>
}

const activeRecoveries = new Map<string, ActiveRecovery>()
const coordinatorFailureCounts = new Map<string, number>()
const recoveryRetryTimers = new Map<string, ReturnType<typeof setTimeout>>()
const stateDbRepairTimes = new Map<string, number>()
let stopping = false

type RecoveryCoordinatorDependencies = {
  isPending: typeof isCodexStateDbBackfillPending
  run: typeof runCodexStateDbBackfillRecovery
  withLock: typeof withCodexBackfillSupervisorLock
}

const defaultCoordinatorDependencies: RecoveryCoordinatorDependencies = {
  isPending: isCodexStateDbBackfillPending,
  run: runCodexStateDbBackfillRecovery,
  withLock: withCodexBackfillSupervisorLock
}

function settleRecoveryEntry(
  key: string,
  codexHomePath: string,
  task: ActiveRecovery['task'],
  summary: CodexStateDbBackfillRecoverySummary | null
): void {
  if (activeRecoveries.get(key)?.task !== task) {
    return
  }
  if (summary?.outcome === 'gave-up') {
    coordinatorFailureCounts.delete(key)
    activeRecoveries.delete(key)
    scheduleRecoveryRetry(key, codexHomePath)
    return
  }
  if (summary?.outcome === 'unreadable') {
    coordinatorFailureCounts.delete(key)
    activeRecoveries.delete(key)
    scheduleRecoveryRetry(key, codexHomePath)
    return
  }
  if (summary === null) {
    const failureCount = (coordinatorFailureCounts.get(key) ?? 0) + 1
    if (failureCount >= RECOVERY_MAX_COORDINATOR_FAILURES) {
      coordinatorFailureCounts.delete(key)
      activeRecoveries.delete(key)
      scheduleRecoveryRetry(key, codexHomePath)
      return
    }
    coordinatorFailureCounts.set(key, failureCount)
  } else {
    coordinatorFailureCounts.delete(key)
  }
  activeRecoveries.delete(key)
}

function clearRecoveryRetryTimer(key: string): void {
  const timer = recoveryRetryTimers.get(key)
  if (!timer) {
    return
  }
  clearTimeout(timer)
  recoveryRetryTimers.delete(key)
}

function scheduleRecoveryRetry(key: string, codexHomePath: string): void {
  if (stopping || recoveryRetryTimers.has(key)) {
    return
  }
  const timer = setTimeout(() => {
    recoveryRetryTimers.delete(key)
    void startCodexStateDbBackfillRecoveryInBackground(codexHomePath)
  }, RECOVERY_SUPERVISOR_RETRY_DELAY_MS)
  timer.unref?.()
  recoveryRetryTimers.set(key, timer)
}

export function startCodexStateDbBackfillRecoveryInBackground(
  codexHomePath: string,
  dependenciesOverride: Partial<RecoveryCoordinatorDependencies> = {}
): Promise<CodexStateDbBackfillRecoverySummary | null> {
  const dependencies = { ...defaultCoordinatorDependencies, ...dependenciesOverride }
  const key = normalizeRuntimePathForComparison(codexHomePath)
  clearRecoveryRetryTimer(key)
  const existing = activeRecoveries.get(key)
  if (existing) {
    return existing.task
  }
  if (stopping) {
    return Promise.resolve(null)
  }
  if (!dependencies.isPending(codexHomePath)) {
    coordinatorFailureCounts.delete(key)
    return Promise.resolve(null)
  }
  const controller = new AbortController()
  let markReady!: () => void
  const ready = new Promise<void>((resolve) => (markReady = resolve))
  const task = dependencies
    .withLock(codexHomePath, controller.signal, async () => {
      console.info(`[codex-state-db-backfill] supervising Codex index at ${codexHomePath}`)
      markReady()
      return await dependencies.run(codexHomePath, controller.signal)
    })
    .catch((error: unknown) => {
      if (!controller.signal.aborted) {
        console.warn('[codex-state-db-backfill] recovery supervisor stopped:', error)
      }
      return null
    })
  void task.finally(markReady)
  activeRecoveries.set(key, { controller, ready, task })
  void task.then((summary) => {
    settleRecoveryEntry(key, codexHomePath, task, summary)
  })
  return task
}

/** Waits only for exact-owner arbitration, never for the potentially long Codex index. */
export async function ensureCodexStateDbBackfillRecoveryStarted(
  codexHomePath: string
): Promise<void> {
  void startCodexStateDbBackfillRecoveryInBackground(codexHomePath)
  await activeRecoveries.get(normalizeRuntimePathForComparison(codexHomePath))?.ready
}

export async function stopCodexStateDbBackfillRecoveries(): Promise<void> {
  stopping = true
  const recoveries = [...activeRecoveries.values()]
  for (const recovery of recoveries) {
    recovery.controller.abort()
  }
  for (const timer of recoveryRetryTimers.values()) {
    clearTimeout(timer)
  }
  recoveryRetryTimers.clear()
  await Promise.allSettled(recoveries.map(({ task }) => task))
}

export const _internals = {
  resetForTests(): void {
    stopping = false
    activeRecoveries.clear()
    coordinatorFailureCounts.clear()
    for (const timer of recoveryRetryTimers.values()) {
      clearTimeout(timer)
    }
    recoveryRetryTimers.clear()
    stateDbRepairTimes.clear()
  }
}
