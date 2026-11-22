import { translate } from '@/i18n/i18n'

export function TabBarQuickCommandHostLoadStatus({
  failed
}: {
  failed: boolean
}): React.JSX.Element {
  return (
    <div className="mt-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-[11px] text-muted-foreground">
      {failed
        ? translate(
            'auto.components.tab.bar.TabBarQuickCommandHostLoadStatus.82e294f3ca',
            'Host unavailable'
          )
        : translate(
            'auto.components.tab.bar.TabBarQuickCommandHostLoadStatus.7c129b08ff',
            'Loading host…'
          )}
    </div>
  )
}
