import { DASHBOARD_MAX_LABEL_LENGTH } from '../../shared/dashboard-snapshot'
import {
  isDashboardBoundedString,
  isOptionalDashboardBoundedString,
  MAX_DASHBOARD_ID_LENGTH
} from './dashboard-payload-bounds'

const ORCHESTRATION_KEYS = new Set([
  'taskId',
  'dispatchId',
  'dispatchStatus',
  'taskTitle',
  'displayName',
  'orchestrationRunId'
])
const DISPATCH_STATUSES = new Set([
  'pending',
  'dispatched',
  'completed',
  'failed',
  'circuit_broken'
])

export function isDashboardOrchestration(value: unknown): boolean {
  if (value === undefined) {
    return true
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return false
  }
  const orchestration = value as Record<string, unknown>
  return (
    Object.keys(orchestration).every((key) => ORCHESTRATION_KEYS.has(key)) &&
    isDashboardBoundedString(orchestration.taskId, MAX_DASHBOARD_ID_LENGTH) &&
    isDashboardBoundedString(orchestration.dispatchId, MAX_DASHBOARD_ID_LENGTH) &&
    (orchestration.dispatchStatus === undefined ||
      (typeof orchestration.dispatchStatus === 'string' &&
        DISPATCH_STATUSES.has(orchestration.dispatchStatus))) &&
    isOptionalDashboardBoundedString(orchestration.taskTitle, DASHBOARD_MAX_LABEL_LENGTH) &&
    isOptionalDashboardBoundedString(orchestration.displayName, DASHBOARD_MAX_LABEL_LENGTH) &&
    isOptionalDashboardBoundedString(
      orchestration.orchestrationRunId,
      MAX_DASHBOARD_ID_LENGTH
    )
  )
}
