import { useMemo } from 'react'
import { AlertTriangle, CheckCircle2, CircleDot, Inbox, Network, Server } from 'lucide-react'
import type { DashboardCard } from '../../../../shared/dashboard-snapshot'
import { translate } from '@/i18n/i18n'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  buildMissionControlProjection,
  type MissionControlTaskState
} from './mission-control-projection'

type MissionControlViewProps = {
  cards: readonly DashboardCard[]
  onOpenTerminal: (card: DashboardCard) => void
  onOpenAgentMap: () => void
}

const TASK_STATE_STYLES: Readonly<Record<MissionControlTaskState, string>> = {
  attention: 'border-agent-question/25 bg-agent-question/15 text-agent-question-text',
  failed: 'border-destructive/20 bg-destructive/10 text-destructive',
  working: 'border-border bg-muted text-foreground',
  completed: 'border-status-success-border bg-status-success-background text-status-success',
  idle: 'border-border bg-background text-muted-foreground'
}

function taskStateLabel(state: MissionControlTaskState): string {
  switch (state) {
    case 'attention':
      return translate('dashboardPopout.mission.state.attention', 'Needs you')
    case 'failed':
      return translate('dashboardPopout.mission.state.failed', 'Failed')
    case 'working':
      return translate('dashboardPopout.mission.state.working', 'Working')
    case 'completed':
      return translate('dashboardPopout.mission.state.completed', 'Completed')
    case 'idle':
      return translate('dashboardPopout.mission.state.idle', 'Idle')
  }
}

function cardContext(card: DashboardCard): string {
  const workspace = `${card.repoName} / ${card.worktreeName}`
  const agent = card.orchestration?.displayName || card.conversationName || card.agentType
  const host =
    card.hostLabel ||
    (card.hostKind && card.hostKind !== 'local' ? card.hostKind.toUpperCase() : undefined)
  return [workspace, agent, host].filter(Boolean).join(' · ')
}

function countLabel(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`
}

export function MissionControlView({
  cards,
  onOpenTerminal,
  onOpenAgentMap
}: MissionControlViewProps): React.JSX.Element {
  const projection = useMemo(() => buildMissionControlProjection(cards), [cards])
  const { summary } = projection

  return (
    <main className="scrollbar-sleek min-h-0 flex-1 overflow-y-auto p-3">
      <div className="mx-auto flex w-full max-w-[1280px] flex-col gap-3">
        <p className="sr-only" role="status" aria-live="polite">
          {countLabel(summary.decisionCount, 'decision needs you', 'decisions need you')}
        </p>

        <section
          aria-label={translate('dashboardPopout.mission.summary', 'Mission summary')}
          className="grid grid-cols-2 gap-2 lg:grid-cols-4"
        >
          {[
            {
              icon: Network,
              value: countLabel(summary.missionCount, 'mission', 'missions'),
              hint: translate('dashboardPopout.mission.summary.live', 'Visible runs')
            },
            {
              icon: Inbox,
              value: countLabel(summary.decisionCount, 'decision', 'decisions'),
              hint: translate('dashboardPopout.mission.summary.decisions', 'Needs you')
            },
            {
              icon: CircleDot,
              value: countLabel(summary.workingTaskCount, 'working task', 'working tasks'),
              hint: translate('dashboardPopout.mission.summary.active', 'In progress')
            },
            {
              icon: CheckCircle2,
              value: countLabel(summary.completedTaskCount, 'completed task', 'completed tasks'),
              hint: translate('dashboardPopout.mission.summary.completed', 'Visible results')
            }
          ].map(({ icon: Icon, value, hint }) => (
            <div key={hint} className="rounded-xl border border-border/60 bg-muted/25 px-3 py-2.5">
              <div className="flex items-center gap-2 text-muted-foreground">
                <Icon aria-hidden="true" className="size-3.5" />
                <span className="text-[10px] font-medium uppercase tracking-[0.06em]">{hint}</span>
              </div>
              <p className="mt-1 text-sm font-semibold tabular-nums">{value}</p>
            </div>
          ))}
        </section>

        <section
          aria-labelledby="decision-inbox-heading"
          className="rounded-xl border border-border/70 bg-card"
        >
          <header className="flex items-center gap-2 border-b border-border/60 px-3 py-2.5">
            <Inbox aria-hidden="true" className="size-4 text-agent-question-text" />
            <h2 id="decision-inbox-heading" className="text-[13px] font-semibold">
              {translate('dashboardPopout.mission.inbox.title', 'Decision Inbox')}
            </h2>
            <Badge variant="secondary" className="ml-auto text-[11px] tabular-nums">
              {projection.decisions.length}
            </Badge>
          </header>
          {projection.decisions.length === 0 ? (
            <p className="px-3 py-5 text-center text-xs text-muted-foreground">
              {translate(
                'dashboardPopout.mission.inbox.empty',
                'No decisions are waiting.'
              )}
            </p>
          ) : (
            <div className="divide-y divide-border/50">
              {projection.decisions.map((decision) => (
                <Button
                  key={decision.id}
                  type="button"
                  variant="ghost"
                  aria-label={translate(
                    'dashboardPopout.mission.inbox.review',
                    'Review decision: {{title}}',
                    { title: decision.title }
                  )}
                  className="group h-auto w-full items-start justify-start rounded-none px-3 py-3 text-left whitespace-normal hover:bg-muted/40 focus-visible:ring-inset"
                  onClick={() => onOpenTerminal(decision.card)}
                >
                  <span
                    className={cn(
                      'mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-lg',
                      decision.priority === 'high'
                        ? 'bg-agent-question/15 text-agent-question-text ring-1 ring-agent-question/25'
                        : 'bg-muted text-muted-foreground'
                    )}
                  >
                    <AlertTriangle aria-hidden="true" className="size-3.5" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block text-[13px] font-medium leading-5">{decision.title}</span>
                    <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                      {cardContext(decision.card)}
                    </span>
                    {decision.options.length > 0 ? (
                      <span className="mt-2 flex flex-wrap gap-1.5" aria-label="Available options">
                        {decision.options.map((option) => (
                          <Badge
                            key={option}
                            variant="outline"
                            className="rounded-md bg-background text-[11px] font-normal text-muted-foreground"
                          >
                            {option}
                          </Badge>
                        ))}
                      </span>
                    ) : null}
                  </span>
                  {decision.unseen ? (
                    <span
                      className="mt-1.5 size-2 shrink-0 rounded-full bg-agent-question"
                      aria-label={translate('dashboardPopout.mission.inbox.new', 'New')}
                    />
                  ) : null}
                </Button>
              ))}
            </div>
          )}
        </section>

        <section aria-labelledby="missions-heading" className="flex flex-col gap-2">
          <div className="flex items-center gap-2 px-0.5">
            <Network aria-hidden="true" className="size-4 text-muted-foreground" />
            <h2 id="missions-heading" className="text-[13px] font-semibold">
              {translate('dashboardPopout.mission.list.title', 'Missions')}
            </h2>
            <span className="text-[11px] text-muted-foreground">
              {translate(
                'dashboardPopout.mission.list.scope',
                'Visible dispatched work'
              )}
            </span>
          </div>

          {projection.missions.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border px-3 py-8 text-center">
              <p className="text-xs text-muted-foreground">
                {translate(
                  'dashboardPopout.mission.list.empty',
                  'No visible mission runs yet.'
                )}
              </p>
              <p className="mt-1 text-[11px] text-muted-foreground">
                {translate(
                  'dashboardPopout.mission.list.emptyHint',
                  'Missions appear when a dispatched agent reports an orchestration run.'
                )}
              </p>
              <Button
                type="button"
                variant="outline"
                size="xs"
                className="mt-3"
                onClick={() => (cards[0] ? onOpenTerminal(cards[0]) : onOpenAgentMap())}
              >
                {cards[0]
                  ? translate(
                      'dashboardPopout.mission.list.openAgent',
                      'Open an agent to dispatch work'
                    )
                  : translate(
                      'dashboardPopout.mission.list.openAgentMap',
                      'Open Agent Map to start an agent'
                    )}
              </Button>
            </div>
          ) : (
            projection.missions.map((mission) => (
              <article key={mission.id} className="rounded-xl border border-border/70 bg-card">
                <header className="border-b border-border/60 px-3 py-3">
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                    <h3 className="text-[13px] font-semibold">{mission.label}</h3>
                    <span className="text-[11px] text-muted-foreground">
                      {countLabel(mission.tasks.length, 'task', 'tasks')} ·{' '}
                      {countLabel(mission.workspaceCount, 'workspace', 'workspaces')} ·{' '}
                      {countLabel(mission.knownHostCount, 'known host', 'known hosts')}
                    </span>
                    <span className="ml-auto flex flex-wrap justify-end gap-1">
                      {(
                        [
                          ['attention', mission.attentionCount],
                          ['failed', mission.failedCount],
                          ['working', mission.workingCount],
                          ['completed', mission.completedCount],
                          ['idle', mission.idleCount]
                        ] as const
                      )
                        .filter(([, count]) => count > 0)
                        .map(([state, count]) => (
                          <Badge
                            key={state}
                            variant="outline"
                            className={cn('text-[10px]', TASK_STATE_STYLES[state])}
                          >
                            {count} {taskStateLabel(state)}
                          </Badge>
                        ))}
                    </span>
                  </div>
                  <div className="mt-2 flex items-center gap-2">
                    <div
                      role="progressbar"
                      aria-label={`${mission.label} progress`}
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={mission.progressPercent}
                      className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"
                    >
                      <div
                        className="h-full rounded-full bg-status-success transition-[width] motion-reduce:transition-none"
                        style={{ width: `${mission.progressPercent}%` }}
                      />
                    </div>
                    <span className="text-[11px] tabular-nums text-muted-foreground">
                      {mission.progressPercent}%
                    </span>
                  </div>
                </header>
                <div className="divide-y divide-border/50">
                  {mission.tasks.map((task) => (
                    <Button
                      key={task.id}
                      type="button"
                      variant="ghost"
                      aria-label={translate(
                        'dashboardPopout.mission.task.open',
                        'Open task: {{title}}',
                        { title: task.title }
                      )}
                      className="h-auto w-full justify-start rounded-none px-3 py-2.5 text-left whitespace-normal hover:bg-muted/40 focus-visible:ring-inset"
                      onClick={() => onOpenTerminal(task.card)}
                    >
                      <Badge
                        variant="outline"
                        className={cn(
                          'w-[72px] shrink-0 rounded-md text-[10px]',
                          TASK_STATE_STYLES[task.state]
                        )}
                      >
                        {taskStateLabel(task.state)}
                      </Badge>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-xs font-medium">{task.title}</span>
                        <span className="block truncate text-[11px] text-muted-foreground">
                          {cardContext(task.card)}
                        </span>
                      </span>
                      {task.attemptCount > 1 ? (
                        <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                          {countLabel(task.attemptCount, 'attempt', 'attempts')}
                        </span>
                      ) : null}
                    </Button>
                  ))}
                </div>
              </article>
            ))
          )}
        </section>

        {summary.unassignedAgentCount > 0 ? (
          <div className="flex items-center gap-2 rounded-lg border border-dashed border-border px-3 py-2 text-[11px] text-muted-foreground">
            <Server aria-hidden="true" className="size-3.5" />
            {countLabel(
              summary.unassignedAgentCount,
              'agent is not linked to a mission run.',
              'agents are not linked to a mission run.'
            )}
          </div>
        ) : null}
      </div>
    </main>
  )
}
