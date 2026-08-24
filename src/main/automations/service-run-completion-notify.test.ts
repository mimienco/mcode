import { describe, expect, it, vi, beforeEach } from 'vitest'

const collectUsageMock = vi.fn()
vi.mock('./run-usage-collection', () => ({
  collectAutomationRunUsage: (input: unknown) => collectUsageMock(input)
}))

import { AutomationService } from './service'
import type {
  Automation,
  AutomationDispatchResult,
  AutomationRun,
  AutomationRunStatus
} from '../../shared/automations-types'
import type { TuiAgent } from '../../shared/tui-agent'

function makeAutomation(): Automation {
  return {
    id: 'auto-1',
    name: 'Nightly review',
    prompt: 'Review yesterday commits',
    precheck: null,
    agentId: 'claude' as TuiAgent,
    projectId: 'repo-1',
    executionTargetType: 'local',
    executionTargetId: '',
    schedulerOwner: 'local_host_service',
    workspaceMode: 'new_per_run',
    workspaceId: null,
    baseBranch: null,
    reuseSession: false,
    timezone: 'UTC',
    rrule: 'FREQ=DAILY',
    dtstart: 0,
    enabled: true,
    nextRunAt: 0,
    missedRunPolicy: 'run_once_within_grace',
    missedRunGraceMinutes: 5,
    createdAt: 0,
    updatedAt: 0
  }
}

function makeRun(overrides?: Partial<AutomationRun>): AutomationRun {
  return {
    id: 'run-1',
    automationId: 'auto-1',
    title: 'Nightly review run',
    scheduledFor: 0,
    status: 'dispatched',
    trigger: 'scheduled',
    workspaceId: null,
    sessionKind: 'terminal',
    chatSessionId: null,
    terminalSessionId: null,
    terminalPaneKey: null,
    terminalPtyId: null,
    outputSnapshot: null,
    precheckResult: null,
    usage: null,
    error: null,
    startedAt: null,
    dispatchedAt: null,
    createdAt: 0,
    ...overrides
  }
}

function makeStore(runs: Map<string, AutomationRun>) {
  return {
    updateAutomation: vi.fn(),
    updateAutomationRun: vi.fn((input: Record<string, unknown>) => {
      const runId = String(input.runId)
      const run = runs.get(runId)
      if (!run) {
        throw new Error(`unknown run ${runId}`)
      }
      Object.assign(run, input)
      return run
    }),
    listAutomationRuns: (automationId: string) =>
      [...runs.values()].filter((entry) => entry.automationId === automationId),
    listAutomations: vi.fn(() => [makeAutomation()])
  }
}

async function markStatus(
  service: AutomationService,
  runId: string,
  status: AutomationRunStatus
): Promise<AutomationRun> {
  return await service.markDispatchResult({
    runId,
    status
  } as unknown as AutomationDispatchResult)
}

/** Mirrors run-usage-collection's real fallback when no provider store exists,
 *  so the `run.usage` early-return behaves exactly like production. */
const UNAVAILABLE_USAGE = {
  status: 'unavailable',
  provider: null,
  model: null,
  inputTokens: null,
  outputTokens: null,
  cacheReadTokens: null,
  cacheWriteTokens: null,
  reasoningOutputTokens: null,
  totalTokens: null,
  estimatedCostUsd: null,
  estimatedCostSource: null,
  providerSessionId: null,
  attribution: null,
  collectedAt: 0,
  unavailableReason: 'usage_not_enabled',
  unavailableMessage: null
}

describe('AutomationService run completion notifications', () => {
  beforeEach(() => {
    collectUsageMock.mockResolvedValue(UNAVAILABLE_USAGE)
  })

  it('notifies exactly once when a run finalizes', async () => {
    const runs = new Map([[makeRun().id, makeRun()]])
    const store = makeStore(runs)
    const notifyRunCompleted = vi.fn()
    const service = new AutomationService(store as never, { notifyRunCompleted })

    await markStatus(service, 'run-1', 'completed')

    expect(notifyRunCompleted).toHaveBeenCalledTimes(1)
    expect(notifyRunCompleted.mock.calls[0][0]).toMatchObject({ id: 'run-1', status: 'completed' })
  })

  it('does not re-notify when an already-finalized run is re-reported', async () => {
    const run = makeRun()
    const runs = new Map([[run.id, run]])
    const notifyRunCompleted = vi.fn()
    const service = new AutomationService(makeStore(runs) as never, { notifyRunCompleted })

    await markStatus(service, run.id, 'completed')
    // Second report carries the usage the first finalize persisted, so the
    // early-return path must skip the notification too.
    await markStatus(service, run.id, 'completed')

    expect(notifyRunCompleted).toHaveBeenCalledTimes(1)
  })

  it('trips the cost circuit-breaker when the estimate exceeds maxCostUsdPerRun', async () => {
    const run = makeRun()
    const runs = new Map([[run.id, run]])
    const store = makeStore(runs)
    const updateAutomation = vi.fn()
    store.updateAutomation = updateAutomation
    const notifyRunCompleted = vi.fn()
    const service = new AutomationService(store as never, {
      notifyRunCompleted
    })
    collectUsageMock.mockResolvedValue({
      status: 'known',
      provider: 'claude',
      model: 'claude-sonnet-5',
      inputTokens: 1,
      outputTokens: 1,
      cacheReadTokens: null,
      cacheWriteTokens: null,
      reasoningOutputTokens: null,
      totalTokens: 2,
      estimatedCostUsd: 5,
      estimatedCostSource: 'api_equivalent',
      providerSessionId: 'session-1',
      attribution: 'provider_session_time_window',
      collectedAt: 0,
      unavailableReason: null,
      unavailableMessage: null
    })

    // Automation fixture carries a $1 cap; make listAutomations return it with the cap.
    const automationWithCap = { ...makeAutomation(), maxCostUsdPerRun: 1 }
    ;(store.listAutomations as ReturnType<typeof vi.fn>).mockImplementation(() => [automationWithCap])

    const final = await markStatus(service, run.id, 'completed')

    expect(final.status).toBe('stopped_cost_cap')
    expect(final.error).toContain('$1.00')
    expect(final.error).toContain('$5.00')
    expect(updateAutomation).toHaveBeenCalledWith('auto-1', { enabled: false })
    expect(notifyRunCompleted).toHaveBeenCalledTimes(1)
    expect(notifyRunCompleted.mock.calls[0][0].status).toBe('stopped_cost_cap')
  })

  it('stays silent for non-final statuses', async () => {
    const runs = new Map([[makeRun().id, makeRun()]])
    const notifyRunCompleted = vi.fn()
    const service = new AutomationService(makeStore(runs) as never, { notifyRunCompleted })

    await markStatus(service, 'run-1', 'dispatching')

    expect(notifyRunCompleted).not.toHaveBeenCalled()
  })
})
