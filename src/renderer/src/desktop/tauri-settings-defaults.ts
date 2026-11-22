import { getDefaultSettings } from '../../../shared/constants'
import type { GlobalSettings } from '../../../shared/global-settings-types'

function fillDefaults(defaults: unknown, saved: unknown): unknown {
  if (defaults !== null && typeof defaults === 'object' && !Array.isArray(defaults)) {
    const fields =
      saved !== null && typeof saved === 'object' && !Array.isArray(saved)
        ? (saved as Record<string, unknown>)
        : {}
    const baseline = defaults as Record<string, unknown>
    return Object.fromEntries(
      [...new Set([...Object.keys(baseline), ...Object.keys(fields)])].map((key) => [
        key,
        fillDefaults(baseline[key], fields[key])
      ])
    )
  }
  return saved === undefined ? structuredClone(defaults) : saved
}

export function resolveTauriSettings(
  document: Record<string, unknown>,
  home: string
): GlobalSettings {
  return fillDefaults(getDefaultSettings(home), document.settings) as GlobalSettings
}
