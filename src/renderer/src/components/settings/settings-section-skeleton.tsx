import { translate } from '@/i18n/i18n'

/** Why: PERF-03 stage B — stable-height placeholder while a section chunk loads;
 * the section header renders outside this fallback so deep-link anchors stay valid. */
export function SettingsSectionSkeleton(): React.JSX.Element {
  return (
    <div
      role="status"
      aria-busy="true"
      className="space-y-3 rounded-lg border border-border/60 bg-muted/20 p-4"
    >
      <span className="sr-only">
        {translate(
          'auto.components.settings.SettingsSectionSkeleton.1a2b3c4d5e',
          'Loading section…'
        )}
      </span>
      <div className="h-4 w-40 animate-pulse rounded bg-muted" />
      <div className="h-3 w-full animate-pulse rounded bg-muted" />
      <div className="h-3 w-4/5 animate-pulse rounded bg-muted" />
      <div className="h-3 w-2/3 animate-pulse rounded bg-muted" />
    </div>
  )
}
