import type { GlobalSettings } from '../../../../shared/global-settings-types'

type GlobalSettingsNestedWriteOptions<TSource> = {
  /** Read inside the chain so each write observes the store after the previous persist reconciled. */
  getSettings: () => TSource | null | undefined
  /** Used only while the store has not hydrated its settings yet. */
  fallback: TSource
  updateSettings: (updates: Partial<GlobalSettings>) => void | Promise<void>
}

/**
 * Serializes full-object replacement writes to nested settings fields so each
 * write rebuilds its payload from the latest state instead of spreading a
 * stale render-time snapshot over a newer one.
 */
export function createGlobalSettingsNestedWriteQueue<TSource>(): (
  options: GlobalSettingsNestedWriteOptions<TSource>
) => (buildUpdate: (settings: TSource) => Partial<GlobalSettings>) => Promise<void> {
  let pendingUpdate: Promise<unknown> = Promise.resolve()

  return ({ getSettings, fallback, updateSettings }) => (buildUpdate) => {
    pendingUpdate = pendingUpdate
      .catch(() => {})
      .then(() => updateSettings(buildUpdate(getSettings() ?? fallback)))
    return pendingUpdate.then(() => undefined)
  }
}
