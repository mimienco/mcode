import { translate } from '@/i18n/i18n'

/** Static non-null Suspense fallback for cold page chunks; no heavy imports. */
export function PageLoadingSurface(): React.JSX.Element {
  return (
    <div
      role="status"
      aria-busy="true"
      aria-live="polite"
      className="flex flex-1 items-center justify-center gap-3 text-muted-foreground"
    >
      <span
        aria-hidden="true"
        className="h-5 w-5 shrink-0 animate-spin rounded-full border-2 border-border border-t-foreground"
      />
      <span className="text-sm">{translate('auto.App.7c1f2d9a40', 'Loading…')}</span>
    </div>
  )
}
