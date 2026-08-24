import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest'

import { AutomationService } from './service'
import type {
  Automation,
  AutomationDispatchResult,
  AutomationRun,
  AutomationRunStatus
} from '../../shared/automations-types'
import type { TuiAgent } from '../../shared/tui-agent'

const collectUsageMock = vi.fn()
vi.mock('./run-usage-collection', () => ({
  collectAutomationRunUsage: (input: unknown) => collectUsageMock(input)
}))

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

function makeRun(id: string, overrides?: Partial<AutomationRun>): AutomationRun {
  return {
    id,
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

/** In-memory store double mirroring the facade semantics the service touches. */
type StoreDouble = {
  runs: Map<string, AutomationRun>
  updateAutomation: Mock
  updateAutomationRun: Mock
  listAutomationRuns: Mock
  listAutomations: Mock
  createAutomationRun: Mock
  linkAutomationRunRetry: Mock
}

function makeStore(): StoreDouble {
  const runs = new Map<string, AutomationRun>()
  let nextId = 1
  return {
    runs,
    updateAutomation: vi.fn(),
    updateAutomationRun: vi.fn((input: Record<string, unknown>) => {
      const run = runs.get(String(input.runId))
      if (!run) {
        throw new Error(`unknown run ${String(input.runId)}`)
      }
      Object.assign(run, input)
      return run
    }),
    listAutomationRuns: vi.fn((automationId: string) =>
      [...runs.values()].filter((entry) => entry.automationId === automationId)
    ),
    listAutomations: vi.fn(() => [makeAutomation()]),
    createAutomationRun: vi.fn(
      (_automation: Automation, _at: number, trigger: 'scheduled') => {
        // Why: mirror the real operation, which registers the fresh run immediately.
        const created = makeRun(`retry-${nextId++}`, { status: 'pending', trigger })
        runs.set(created.id, created)
        return created
      }
    ),
    linkAutomationRunRetry: vi.fn(
      (input: { failedRunId: string; retryRunId: string; title: string }) => {
        const retry = runs.get(input.retryRunId)
        if (!retry) {
          throw new Error('unknown retry run')
        }
        retry.retryOfRunId = input.failedRunId
        retry.title = input.title
        return retry
      }
    )
  }
}

async function markStatus(
  service: AutomationService,
  runId: string,
  status: AutomationRunStatus
): Promise<void> {
  await service.markDispatchResult({ runId, status } as unknown as AutomationDispatchResult)
}

describe('AutomationService bounded retry', () => {
  let store: StoreDouble
  let notifyRunCompleted: Mock

  beforeEach(() => {
    store = makeStore()
    const seeded = makeRun('run-1')
    store.runs.set(seeded.id, seeded)
    notifyRunCompleted = vi.fn()
    collectUsageMock.mockResolvedValue({
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
    })
  })

  function makeService(): AutomationService {
    return new AutomationService(store as never, { notifyRunCompleted })
  }

  it('creates exactly one linked retry run after a scheduled failure', async () => {
    const service = makeService()
    await markStatus(service, 'run-1', 'dispatch_failed')

    expect(store.createAutomationRun).toHaveBeenCalledTimes(1)
    expect(store.linkAutomationRunRetry).toHaveBeenCalledWith({
      failedRunId: 'run-1',
      retryRunId: 'retry-1',
      title: 'Retry · Nightly review run'
    })
    const retry = store.runs.get('retry-1')
    expect(retry?.retryOfRunId).toBe('run-1')
    // Why: the bare store double cannot resolve a run target, so scheduling
    // throws — the contract under test is that this crash SURFACES as the
    // retry's own failure instead of vanishing into an unobserved promise.
    expect(retry?.status).toBe('dispatch_failed')
    expect(retry?.error).toBe('Retry dispatch failed.')
  })

  it('never retries the retry itself', async () => {
    const service = makeService()
    await markStatus(service, 'run-1', 'dispatch_failed')

    const attemptsBefore = store.createAutomationRun.mock.calls.length
    await markStatus(service, 'retry-1', 'dispatch_failed')

    expect(store.createAutomationRun.mock.calls.length).toBe(attemptsBefore)
  })

  it('does not retry manually triggered failures', async () => {
    const manual = makeRun('manual-1', { trigger: 'manual' })
    store.runs.set(manual.id, manual)
    const service = makeService()

    await markStatus(service, 'manual-1', 'dispatch_failed')

    expect(store.createAutomationRun).not.toHaveBeenCalled()
  })
})
