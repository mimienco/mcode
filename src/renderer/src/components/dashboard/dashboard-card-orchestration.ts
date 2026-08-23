import type { AgentStatusOrchestrationContext } from '../../../../shared/agent-status-types'
import type { DashboardCardOrchestration } from '../../../../shared/dashboard-snapshot'
import { boundedLabelOrUndefined } from './dashboard-card-labels'

const MAX_DASHBOARD_ORCHESTRATION_ID_LENGTH = 4_096

function isBoundedId(value: string): boolean {
  return value.length > 0 && value.length <= MAX_DASHBOARD_ORCHESTRATION_ID_LENGTH
}

/** Narrows live runtime context to the display-only dashboard contract. */
export function dashboardCardOrchestration(
  orchestration: AgentStatusOrchestrationContext
): DashboardCardOrchestration | undefined {
  if (!isBoundedId(orchestration.taskId) || !isBoundedId(orchestration.dispatchId)) {
    return undefined
  }
  const orchestrationRunId =
    orchestration.orchestrationRunId && isBoundedId(orchestration.orchestrationRunId)
      ? orchestration.orchestrationRunId
      : undefined
  return {
    taskId: orchestration.taskId,
    dispatchId: orchestration.dispatchId,
    ...(orchestration.dispatchStatus ? { dispatchStatus: orchestration.dispatchStatus } : {}),
    ...(orchestration.taskTitle
      ? { taskTitle: boundedLabelOrUndefined(orchestration.taskTitle) }
      : {}),
    ...(orchestration.displayName
      ? { displayName: boundedLabelOrUndefined(orchestration.displayName) }
      : {}),
    ...(orchestrationRunId ? { orchestrationRunId } : {})
  }
}
