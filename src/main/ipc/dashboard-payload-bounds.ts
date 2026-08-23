export const MAX_DASHBOARD_ID_LENGTH = 4_096

export function isDashboardBoundedString(
  value: unknown,
  maxLength: number,
  allowEmpty = false
): value is string {
  return typeof value === 'string' && value.length <= maxLength && (allowEmpty || value.length > 0)
}

export function isOptionalDashboardBoundedString(value: unknown, maxLength: number): boolean {
  return value === undefined || isDashboardBoundedString(value, maxLength, true)
}
