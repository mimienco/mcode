import React from 'react'
import { Minus, Plus } from 'lucide-react'
import { AgentIcon } from '@/lib/agent-catalog'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import type { QuickLaunchAgentMenuEntry } from '@/components/tab-bar/QuickLaunchButton'
import type { TuiAgent } from '../../../../shared/tui-agent'
import {
  getEmptyStateAgentChannelColor,
  getEmptyStateAgentLabel
} from './tab-group-empty-state-agent-metadata'

export function TabGroupEmptyStateAgentGrid({
  launchEntries,
  sessionCounts,
  maxSessions,
  onSetSessionCount
}: {
  launchEntries: QuickLaunchAgentMenuEntry[]
  sessionCounts: Record<string, number>
  maxSessions: number
  onSetSessionCount: (agent: TuiAgent, count: number) => void
}): React.JSX.Element {
  return (
    <div className="grid gap-2 @min-[560px]/empty-state:grid-cols-2 @min-[860px]/empty-state:gap-4">
      {launchEntries.map(({ agent }) => {
        const count = sessionCounts[agent] ?? 0
        const label = getEmptyStateAgentLabel(agent)
        return (
          <div
            key={agent}
            data-agent-row={agent}
            data-agent-selected={count > 0 ? '' : undefined}
            className={`tab-group-empty-state-agent-card flex min-w-0 rounded-lg border bg-card transition-colors ${
              count > 0 ? 'border-ai-action-accent/70' : 'border-border'
            }`}
          >
            <div className="tab-group-empty-state-agent-identity flex h-10 w-full min-w-0 items-center justify-start gap-2 px-2 text-left">
              <span
                aria-hidden
                data-agent-channel-led={agent}
                className="size-2 shrink-0 rounded-full @min-[860px]/empty-state:size-2.5"
                style={{ background: getEmptyStateAgentChannelColor(agent) }}
              />
              <span className="tab-group-empty-state-agent-icon shrink-0 text-foreground">
                <AgentIcon agent={agent} size={24} />
              </span>
              <span className="tab-group-empty-state-agent-label min-w-0 flex-1 whitespace-nowrap text-[10px] font-semibold text-foreground @min-[360px]/empty-state:text-sm @min-[860px]/empty-state:text-base">
                {label}
              </span>
            </div>

            <div
              role="group"
              aria-label={translate(
                'auto.components.tab.group.TabGroupEmptyState.sessionCountLabel',
                'Sessions to open'
              )}
              className="tab-group-empty-state-agent-count mb-1 mr-1 flex shrink-0 self-end items-center gap-0.5 text-muted-foreground"
            >
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={translate(
                  'auto.components.tab.group.TabGroupEmptyState.decrementSessions',
                  'Fewer {{value0}} sessions',
                  { value0: label }
                )}
                disabled={count <= 0}
                data-agent-count-decrement={agent}
                className="text-muted-foreground hover:text-foreground @min-[860px]/empty-state:size-8"
                onClick={() => {
                  onSetSessionCount(agent, count - 1)
                }}
              >
                <Minus />
              </Button>
              <span
                data-agent-count={agent}
                className={`readout w-7 text-center text-xs @min-[860px]/empty-state:w-8 @min-[860px]/empty-state:text-sm ${
                  count > 0 ? 'font-medium text-ai-action-accent' : 'text-muted-foreground'
                }`}
              >
                {count}
              </span>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label={translate(
                  'auto.components.tab.group.TabGroupEmptyState.incrementSessions',
                  'More {{value0}} sessions',
                  { value0: label }
                )}
                disabled={count >= maxSessions}
                data-agent-count-increment={agent}
                className="tab-group-empty-state-agent-plus border-border bg-background text-foreground shadow-sm hover:border-muted-foreground/40 hover:bg-accent @min-[860px]/empty-state:size-9"
                onClick={() => {
                  onSetSessionCount(agent, count + 1)
                }}
              >
                <Plus />
              </Button>
            </div>
          </div>
        )
      })}
    </div>
  )
}
