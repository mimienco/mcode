import React from 'react'
import { AgentIcon } from '@/lib/agent-catalog'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import type { QuickLaunchAgentMenuEntry } from '@/components/tab-bar/QuickLaunchButton'
import type { TuiAgent } from '../../../../shared/tui-agent'
import { getEmptyStateAgentLabel } from './tab-group-empty-state-agent-metadata'

export function TabGroupEmptyStateInstallList({
  installEntries,
  onInstall
}: {
  installEntries: QuickLaunchAgentMenuEntry[]
  onInstall: (agent: TuiAgent) => void
}): React.JSX.Element | null {
  if (installEntries.length === 0) {
    return null
  }

  return (
    <div className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-card">
      {installEntries.map(({ agent }) => (
        <div
          key={agent}
          className="flex min-w-0 items-center gap-3 px-3 py-2.5 @min-[860px]/empty-state:h-16 @min-[860px]/empty-state:px-5"
        >
          <AgentIcon agent={agent} size={24} />
          <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-muted-foreground @min-[860px]/empty-state:text-base">
            {getEmptyStateAgentLabel(agent)}
          </span>
          <Button
            type="button"
            variant="outline"
            size="xs"
            className="border-ai-action-accent/70 text-ai-action-accent @min-[860px]/empty-state:h-10 @min-[860px]/empty-state:px-5 @min-[860px]/empty-state:text-sm"
            data-agent-install-row={agent}
            onClick={() => {
              onInstall(agent)
            }}
          >
            {translate('auto.components.settings.AgentsPane.f95b5c79b8', 'Install')}
          </Button>
        </div>
      ))}
    </div>
  )
}
