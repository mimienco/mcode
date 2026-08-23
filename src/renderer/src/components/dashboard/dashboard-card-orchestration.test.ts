import { describe, expect, it } from 'vitest'
import { DASHBOARD_MAX_LABEL_LENGTH } from '../../../../shared/dashboard-snapshot'
import { dashboardCardOrchestration } from './dashboard-card-orchestration'

describe('dashboardCardOrchestration', () => {
  it('clips display copy while preserving exact bounded identities', () => {
    const orchestration = dashboardCardOrchestration({
      taskId: 'task-1',
      dispatchId: 'dispatch-1',
      taskTitle: 't'.repeat(DASHBOARD_MAX_LABEL_LENGTH + 20),
      displayName: 'n'.repeat(DASHBOARD_MAX_LABEL_LENGTH + 20),
      orchestrationRunId: 'run-1'
    })

    expect(orchestration).toMatchObject({
      taskId: 'task-1',
      dispatchId: 'dispatch-1',
      orchestrationRunId: 'run-1'
    })
    expect(orchestration?.taskTitle).toHaveLength(DASHBOARD_MAX_LABEL_LENGTH)
    expect(orchestration?.displayName).toHaveLength(DASHBOARD_MAX_LABEL_LENGTH)
  })

  it('omits unsafe required identities and an independently unsafe run identity', () => {
    expect(
      dashboardCardOrchestration({ taskId: 'x'.repeat(4_097), dispatchId: 'dispatch-1' })
    ).toBeUndefined()
    expect(
      dashboardCardOrchestration({
        taskId: 'task-1',
        dispatchId: 'dispatch-1',
        orchestrationRunId: 'x'.repeat(4_097)
      })
    ).toEqual({ taskId: 'task-1', dispatchId: 'dispatch-1' })
  })
})
