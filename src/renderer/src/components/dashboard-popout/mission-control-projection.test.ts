import { describe, expect, it } from 'vitest'
import type { DashboardCard } from '../../../../shared/dashboard-snapshot'
import { buildMissionControlProjection } from './mission-control-projection'

function card(overrides: Partial<DashboardCard> = {}): DashboardCard {
  return {
    paneKey: 'tab-1:leaf-1',
    ptyId: 'pty-1',
    agentType: 'codex',
    bucket: 'working',
    dotState: 'working',
    task: 'Implement Mission Control',
    repoId: 'repo-1',
    worktreeId: 'worktree-1',
    tabId: 'tab-1',
    leafId: 'leaf-1',
    repoName: 'MCode',
    worktreeName: 'mission-control',
    hostKind: 'local',
    executionHostId: 'local',
    startedAt: 100,
    finishedAt: null,
    stateChangedAt: 200,
    unseen: false,
    ...overrides
  }
}

describe('buildMissionControlProjection', () => {
  it('projects a plain attention prompt as one operator decision', () => {
    const pending = card({
      bucket: 'attention',
      dotState: 'waiting',
      unseen: true,
      askSummary: 'Approve the database migration?'
    })

    const projection = buildMissionControlProjection([pending])

    expect(projection.decisions).toEqual([
      expect.objectContaining({
        id: pending.paneKey,
        title: 'Approve the database migration?',
        options: [],
        priority: 'normal',
        unseen: true,
        card: pending
      })
    ])
  })

  it('turns a JSON interactive prompt into a readable question and options', () => {
    const pending = card({
      bucket: 'attention',
      dotState: 'blocked',
      askSummary: JSON.stringify({
        questions: [
          {
            header: 'Database',
            question: 'Which migration strategy should we use?',
            options: [
              { label: 'Expand and contract', description: 'Keeps old clients working.' },
              { label: 'Direct migration', description: 'Faster but requires downtime.' }
            ]
          }
        ]
      })
    })

    const projection = buildMissionControlProjection([pending])

    expect(projection.decisions[0]).toMatchObject({
      title: 'Which migration strategy should we use?',
      options: ['Expand and contract', 'Direct migration'],
      priority: 'high'
    })
  })

  it('bounds structured prompt copy before it reaches the dashboard', () => {
    const pending = card({
      bucket: 'attention',
      askSummary: JSON.stringify({
        question: 'q'.repeat(300),
        options: Array.from({ length: 12 }, (_, index) => `${index}-${'x'.repeat(150)}`)
      })
    })

    const [decision] = buildMissionControlProjection([pending]).decisions

    expect(decision.title).toHaveLength(240)
    expect(decision.options).toHaveLength(8)
    expect(decision.options.every((option) => option.length <= 120)).toBe(true)
  })

  it('falls back to the task instead of exposing malformed structured prompt data', () => {
    const pending = card({
      bucket: 'attention',
      task: 'Deploy safely',
      askSummary: '{"questions":['
    })

    expect(buildMissionControlProjection([pending]).decisions[0].title).toBe('Deploy safely')
  })

  it('orders decisions by unseen, blocker priority, recency, and stable identity', () => {
    const seenBlocker = card({
      paneKey: 'seen-blocker',
      bucket: 'attention',
      dotState: 'blocked',
      unseen: false,
      stateChangedAt: 900
    })
    const unseenWaiting = card({
      paneKey: 'unseen-waiting',
      bucket: 'attention',
      dotState: 'waiting',
      unseen: true,
      stateChangedAt: 800
    })
    const unseenBlockerOlder = card({
      paneKey: 'unseen-blocker-b',
      bucket: 'attention',
      dotState: 'blocked',
      unseen: true,
      stateChangedAt: 700
    })
    const unseenBlockerNewer = card({
      paneKey: 'unseen-blocker-a',
      bucket: 'attention',
      dotState: 'blocked',
      unseen: true,
      stateChangedAt: 750
    })

    const ids = buildMissionControlProjection([
      seenBlocker,
      unseenWaiting,
      unseenBlockerOlder,
      unseenBlockerNewer
    ]).decisions.map((decision) => decision.id)

    expect(ids).toEqual([
      'unseen-blocker-a',
      'unseen-blocker-b',
      'unseen-waiting',
      'seen-blocker'
    ])
  })

  it('groups visible dispatch attempts into missions and logical tasks', () => {
    const needsDecision = card({
      paneKey: 'tab-1:leaf-new',
      bucket: 'attention',
      dotState: 'blocked',
      stateChangedAt: 500,
      askSummary: 'Choose a rollout strategy',
      orchestration: {
        taskId: 'task-1',
        dispatchId: 'dispatch-new',
        dispatchStatus: 'dispatched',
        taskTitle: 'Plan rollout',
        orchestrationRunId: 'run-123456789'
      }
    })
    const olderAttempt = card({
      paneKey: 'tab-1:leaf-old',
      bucket: 'done',
      dotState: 'done',
      stateChangedAt: 300,
      orchestration: {
        taskId: 'task-1',
        dispatchId: 'dispatch-old',
        dispatchStatus: 'failed',
        taskTitle: 'Plan rollout',
        orchestrationRunId: 'run-123456789'
      }
    })
    const completed = card({
      paneKey: 'tab-2:leaf-2',
      bucket: 'done',
      dotState: 'done',
      stateChangedAt: 450,
      orchestration: {
        taskId: 'task-2',
        dispatchId: 'dispatch-2',
        dispatchStatus: 'completed',
        taskTitle: 'Verify rollout',
        orchestrationRunId: 'run-123456789'
      }
    })
    const idle = card({
      paneKey: 'tab-2:leaf-idle',
      bucket: 'idle',
      dotState: 'idle',
      stateChangedAt: 400,
      orchestration: {
        taskId: 'task-3',
        dispatchId: 'dispatch-3',
        taskTitle: 'Archive rollout notes',
        orchestrationRunId: 'run-123456789'
      }
    })
    const unassigned = card({ paneKey: 'tab-3:leaf-3' })

    const projection = buildMissionControlProjection([
      olderAttempt,
      completed,
      idle,
      unassigned,
      needsDecision
    ])

    expect(projection.summary).toEqual({
      missionCount: 1,
      decisionCount: 1,
      workingTaskCount: 0,
      completedTaskCount: 1,
      unassignedAgentCount: 1
    })
    expect(projection.missions).toHaveLength(1)
    expect(projection.missions[0]).toMatchObject({
      id: 'run-123456789',
      label: 'Mission run-1234',
      decisionCount: 1,
      attentionCount: 1,
      workingCount: 0,
      failedCount: 0,
      completedCount: 1,
      idleCount: 1,
      progressPercent: 33,
      workspaceCount: 1,
      knownHostCount: 1
    })
    expect(projection.missions[0].tasks).toEqual([
      expect.objectContaining({
        id: 'task-1',
        title: 'Plan rollout',
        state: 'attention',
        attemptCount: 2,
        card: needsDecision
      }),
      expect.objectContaining({
        id: 'task-2',
        title: 'Verify rollout',
        state: 'completed',
        attemptCount: 1,
        card: completed
      }),
      expect.objectContaining({
        id: 'task-3',
        state: 'idle',
        card: idle
      })
    ])
  })

  it('uses the documented state precedence when card and dispatch signals conflict', () => {
    const workingWithCompletedDispatch = card({
      paneKey: 'working-completed',
      bucket: 'working',
      orchestration: {
        taskId: 'task-working',
        dispatchId: 'dispatch-working',
        dispatchStatus: 'completed',
        orchestrationRunId: 'run-conflicts'
      }
    })
    const doneWithDispatchedStatus = card({
      paneKey: 'done-dispatched',
      bucket: 'done',
      dotState: 'done',
      orchestration: {
        taskId: 'task-done',
        dispatchId: 'dispatch-done',
        dispatchStatus: 'dispatched',
        orchestrationRunId: 'run-conflicts'
      }
    })

    const states = buildMissionControlProjection([
      workingWithCompletedDispatch,
      doneWithDispatchedStatus
    ]).missions[0].tasks.map((task) => task.state)

    expect(states).toEqual(['working', 'working'])
  })

  it('orders equally active missions by newest activity rather than task volume', () => {
    const olderBusyCards = ['a', 'b', 'c'].map((suffix) =>
      card({
        paneKey: `older-${suffix}`,
        stateChangedAt: 100,
        orchestration: {
          taskId: `older-task-${suffix}`,
          dispatchId: `older-dispatch-${suffix}`,
          dispatchStatus: 'dispatched',
          orchestrationRunId: 'run-older'
        }
      })
    )
    const newerCard = card({
      paneKey: 'newer',
      stateChangedAt: 200,
      orchestration: {
        taskId: 'newer-task',
        dispatchId: 'newer-dispatch',
        dispatchStatus: 'dispatched',
        orchestrationRunId: 'run-newer'
      }
    })

    const runIds = buildMissionControlProjection([...olderBusyCards, newerCard]).missions.map(
      (mission) => mission.id
    )

    expect(runIds).toEqual(['run-newer', 'run-older'])
  })
})
