import type { WebContents } from 'electron'
import type { Store } from '../persistence'
import {
  isFinalAutomationRunStatus,
  type Automation,
  type AutomationDispatchRequest,
  type AutomationDispatchResult,
  type AutomationPrecheckResult,
  type AutomationRun
} from '../../shared/automations-types'
import type { ClaudeUsageStore } from '../claude-usage/store'
import type { CodexUsageStore } from '../codex-usage/store'
import { runAutomationPrecheck } from './precheck-runner'
import { resolveAutomationRunTarget, type AutomationRunTargetResult } from './run-target-resolution'
import { collectAutomationRunUsage } from './run-usage-collection'
import type { HeadlessAutomationDispatcher } from './headless-dispatch'
import { clearAutomationDispatchTokens, createAutomationDispatchToken } from './dispatch-tokens'
import {
  didAutomationPrecheckPass,
  formatAutomationPrecheckFailure
} from '../../shared/automation-precheck'

const DEFAULT_TICK_MS = 60 * 1000

export class AutomationService {
  private readonly store: Store
  private readonly tickMs: number
  private timer: ReturnType<typeof setInterval> | null = null
  private webContents: WebContents | null = null
  private rendererReady = false
  private evaluating = false
  private readonly claudeUsage: ClaudeUsageStore | null
  private readonly codexUsage: CodexUsageStore | null
  private readonly allowRemoteHostScheduling: boolean
  private readonly headlessDispatcher: HeadlessAutomationDispatcher | null
  /** Why: optional so tests stay hermetic; production wires the shared notification dispatch handler. */
  private readonly notifyRunCompleted?: (run: AutomationRun) => void

  constructor(
    store: Store,
    opts: {
      tickMs?: number
      claudeUsage?: ClaudeUsageStore
      codexUsage?: CodexUsageStore
      allowRemoteHostScheduling?: boolean
      headlessDispatcher?: HeadlessAutomationDispatcher
      notifyRunCompleted?: (run: AutomationRun) => void
    } = {}
  ) {
    this.store = store
    this.tickMs = opts.tickMs ?? DEFAULT_TICK_MS
    this.claudeUsage = opts.claudeUsage ?? null
    this.codexUsage = opts.codexUsage ?? null
    this.allowRemoteHostScheduling = opts.allowRemoteHostScheduling ?? false
    this.headlessDispatcher = opts.headlessDispatcher ?? null
    this.notifyRunCompleted = opts.notifyRunCompleted
  }

  setWebContents(webContents: WebContents | null): void {
    this.webContents = webContents
    this.rendererReady = false
  }

  setRendererReady(): void {
    this.rendererReady = true
    void this.evaluateDueRuns().catch(() => {})
  }

  start(): void {
    if (this.timer) {
      return
    }
    this.timer = setInterval(() => {
      void this.evaluateDueRuns().catch(() => {})
    }, this.tickMs)
    // Why: headless serve never gets a renderer-ready IPC, but due runs still
    // need the same startup catch-up pass desktop gets after renderer attach.
    if (this.rendererReady || this.headlessDispatcher) {
      void this.evaluateDueRuns().catch(() => {})
    }
  }

  stop(): void {
    if (!this.timer) {
      return
    }
    clearInterval(this.timer)
    this.timer = null
  }

  async runNow(automationId: string): Promise<AutomationRun> {
    const automation = this.store.listAutomations().find((entry) => entry.id === automationId)
    if (!automation) {
      throw new Error('Automation not found.')
    }
    const run = this.store.createAutomationRun(automation, Date.now(), 'manual')
    return await this.requestDispatch(automation, run)
  }

  async runPrecheck(automationId: string, runId: string): Promise<AutomationPrecheckResult | null> {
    const automation = this.store.listAutomations().find((entry) => entry.id === automationId)
    if (!automation) {
      throw new Error('Automation not found.')
    }
    const run = this.store.listAutomationRuns(automationId).find((entry) => entry.id === runId)
    if (!run) {
      throw new Error('Automation run not found.')
    }
    if (run.trigger !== 'scheduled' || !automation.precheck) {
      return null
    }
    const target = resolveAutomationRunTarget(this.store, automation, {
      allowRemoteHostScheduling: this.allowRemoteHostScheduling
    })
    if (!target.ok) {
      return {
        command: automation.precheck.command,
        exitCode: null,
        timedOut: false,
        durationMs: 0,
        stdout: '',
        stderr: '',
        stdoutTruncated: false,
        stderrTruncated: false,
        error: target.error,
        startedAt: Date.now(),
        completedAt: Date.now()
      }
    }
    return await runAutomationPrecheck({
      precheck: automation.precheck,
      target:
        automation.executionTargetType === 'ssh'
          ? { type: 'ssh', cwd: target.cwd, connectionId: automation.executionTargetId }
          : { type: 'local', cwd: target.cwd }
    })
  }

  async markDispatchResult(result: AutomationDispatchResult): Promise<AutomationRun> {
    const run = this.store.updateAutomationRun(result)
    clearAutomationDispatchTokens(run.automationId, run.id)
    if (!isFinalAutomationRunStatus(run.status)) {
      return run
    }
    // Why: the renderer's mark-completed effect can re-fire for the same run
    // before refresh() flips its status snapshot off 'dispatched'. Re-running
    // collectRunUsage advances the attribution window and can rewrite an
    // already-collected 'known' usage to 'unavailable'/'ambiguous_session'.
    if (run.usage) {
      return run
    }
    const automation = this.store.listAutomations().find((entry) => entry.id === run.automationId)
    const usage = await collectAutomationRunUsage({
      automation,
      run,
      claudeUsage: this.claudeUsage,
      codexUsage: this.codexUsage
    })
    // Why: the run is final during the await above, so a concurrent create-time
    // retention prune may have evicted it — the usage write must not throw then.
    if (!this.store.listAutomationRuns(run.automationId).some((entry) => entry.id === run.id)) {
      return run
    }
    const updated = this.store.updateAutomationRun({
      runId: run.id,
      status: run.status,
      workspaceId: run.workspaceId,
      terminalSessionId: run.terminalSessionId,
      usage,
      error: run.error
    })
    let final = updated
    if (
      automation?.maxCostUsdPerRun != null &&
      usage.status === 'known' &&
      usage.estimatedCostUsd != null &&
      usage.estimatedCostUsd > automation.maxCostUsdPerRun
    ) {
      // Why: cost circuit-breaker — pause scheduled runs until a human re-enables.
      this.store.updateAutomation(run.automationId, { enabled: false })
      final = this.store.updateAutomationRun({
        runId: run.id,
        status: 'stopped_cost_cap',
        workspaceId: run.workspaceId,
        terminalSessionId: run.terminalSessionId,
        usage,
        error: `Cost cap $${automation.maxCostUsdPerRun.toFixed(2)} exceeded by estimate $${usage.estimatedCostUsd.toFixed(2)}; scheduled runs paused.`
      })
    }
    // Why: bounded retry (o8-ladder-lite) — exactly one automatic re-dispatch per
    // failed scheduled run; the retry itself carries `retryOfRunId`, so a second
    // failure can never spawn a third attempt. Manual runs stay human-owned.
    if (
      final.status === 'dispatch_failed' &&
      !final.retryOfRunId &&
      final.trigger === 'scheduled' &&
      automation
    ) {
      const alreadyRetried = this.store
        .listAutomationRuns(run.automationId)
        .some((entry) => entry.retryOfRunId === final.id)
      if (!alreadyRetried) {
        const retry = this.store.createAutomationRun(automation, Date.now(), 'scheduled')
        const linked = this.store.linkAutomationRunRetry({
          failedRunId: final.id,
          retryRunId: retry.id,
          title: `Retry · ${retry.title}`
        })
        try {
          const dispatchedRetry = await this.requestDispatch(automation, linked)
          // Why: skipped_* settles synchronously here (no window/dispatcher), so
          // announce it now — otherwise the retry's own finalize never notifies.
          if (isFinalAutomationRunStatus(dispatchedRetry.status)) {
            this.notifyRunCompleted?.(dispatchedRetry)
          }
        } catch {
          // Why: a scheduling crash must surface as the retry's own failure,
          // never vanish into an unobserved promise.
          this.store.updateAutomationRun({
            runId: linked.id,
            status: 'dispatch_failed',
            workspaceId: null,
            terminalSessionId: null,
            error: 'Retry dispatch failed.'
          })
        }
      }
    }
    // Why: announce only after the persisted finalize, so a failed write never announces a phantom completion.
    this.notifyRunCompleted?.(final)
    return final
  }

  private async evaluateDueRuns(): Promise<void> {
    if (this.evaluating) {
      return
    }
    this.evaluating = true
    try {
      const now = Date.now()
      for (const automation of this.store.listAutomations()) {
        if (!automation.enabled || automation.nextRunAt > now) {
          continue
        }
        try {
          await this.evaluateAutomation(automation, now)
        } catch (error) {
          // Why: a throw before advanceAutomationNextRun would otherwise
          // abort the whole pass, starve every later automation, and re-fire
          // the same failure every tick.
          console.error('automation-evaluation-failed', automation.id, error)
          this.store.advanceAutomationNextRun(automation.id, now)
        }
      }
    } finally {
      this.evaluating = false
    }
  }

  private async evaluateAutomation(automation: Automation, now: number): Promise<void> {
    const scheduledFor = this.store.getLatestAutomationOccurrence(automation, now)
    if (scheduledFor === null) {
      this.store.advanceAutomationNextRun(automation.id, now)
      return
    }
    const run = this.store.createAutomationRun(automation, scheduledFor)
    const graceMs = automation.missedRunGraceMinutes * 60 * 1000
    if (now - scheduledFor > graceMs) {
      this.store.updateAutomationRun({
        runId: run.id,
        status: 'skipped_missed',
        workspaceId: automation.workspaceId,
        error: 'MCode was unavailable during the missed-run grace window.'
      })
      this.store.advanceAutomationNextRun(automation.id, now)
      return
    }

    await this.requestDispatch(automation, run)
    this.store.advanceAutomationNextRun(automation.id, now)
  }

  private async requestDispatch(
    automation: Automation,
    run: AutomationRun
  ): Promise<AutomationRun> {
    const target = resolveAutomationRunTarget(this.store, automation, {
      allowRemoteHostScheduling: this.allowRemoteHostScheduling
    })
    if (!target.ok) {
      return this.store.updateAutomationRun({
        runId: run.id,
        status: 'skipped_unavailable',
        workspaceId: automation.workspaceId,
        error: target.error
      })
    }
    const webContents = this.webContents
    if (!webContents || webContents.isDestroyed() || !this.rendererReady) {
      if (this.headlessDispatcher) {
        return await this.requestHeadlessDispatch(automation, run, target)
      }
      return this.store.updateAutomationRun({
        runId: run.id,
        status: 'skipped_unavailable',
        workspaceId: automation.workspaceId,
        error: 'No MCode window was available to launch the automation.'
      })
    }
    const updated = this.store.updateAutomationRun({
      runId: run.id,
      status: 'dispatching',
      workspaceId: automation.workspaceId,
      error: null
    })
    const payload: AutomationDispatchRequest = {
      automation,
      run: updated,
      dispatchToken: createAutomationDispatchToken(automation.id, updated.id)
    }
    webContents.send('automations:dispatchRequested', payload)
    return updated
  }

  private async requestHeadlessDispatch(
    automation: Automation,
    run: AutomationRun,
    target: Extract<AutomationRunTargetResult, { ok: true }>
  ): Promise<AutomationRun> {
    const precheckResult =
      run.trigger === 'scheduled' && automation.precheck
        ? await this.runPrecheck(automation.id, run.id)
        : null
    if (precheckResult && !didAutomationPrecheckPass(precheckResult)) {
      return this.store.updateAutomationRun({
        runId: run.id,
        status: 'skipped_precheck',
        workspaceId: automation.workspaceId,
        precheckResult,
        error: formatAutomationPrecheckFailure(precheckResult)
      })
    }
    try {
      const launch = await this.headlessDispatcher!({ automation, run, target })
      const launchRunTarget = {
        workspaceId: launch.workspaceId,
        workspaceDisplayName: launch.workspaceDisplayName ?? null,
        terminalSessionId: launch.terminalSessionId,
        terminalPaneKey: launch.terminalPaneKey ?? null,
        terminalPtyId: launch.terminalPtyId ?? null
      }
      const updated = this.store.updateAutomationRun({
        runId: run.id,
        status: 'dispatched',
        ...launchRunTarget,
        error: null
      })
      if (launch.completion) {
        void launch.completion
          .then((completion) =>
            this.markDispatchResult({
              runId: run.id,
              status: completion.status,
              ...launchRunTarget,
              precheckResult,
              outputSnapshot: completion.outputSnapshot ?? null,
              error: completion.error ?? null
            })
          )
          .catch((error) =>
            this.markDispatchResult({
              runId: run.id,
              status: 'dispatch_failed',
              ...launchRunTarget,
              error: error instanceof Error ? error.message : String(error)
            })
          )
      }
      return updated
    } catch (error) {
      return this.store.updateAutomationRun({
        runId: run.id,
        status: 'dispatch_failed',
        workspaceId: automation.workspaceId,
        error: error instanceof Error ? error.message : String(error)
      })
    }
  }
}
