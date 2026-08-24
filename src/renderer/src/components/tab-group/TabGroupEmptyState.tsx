import React, { useCallback, useMemo, useState } from 'react'
import { ArrowRight, FileText, Globe, Minus, Plus, Settings2, Smartphone, SquareTerminal } from 'lucide-react'
import { toast } from 'sonner'
import { AgentIcon, getAgentCatalog } from '@/lib/agent-catalog'
import { launchAgentInNewTab } from '@/lib/launch-agent-in-new-tab'
import { useAgentDetectionTargetForWorktree } from '@/hooks/useAgentDetectionTarget'
import { useDetectedAgents } from '@/hooks/useDetectedAgents'
import { useAppStore } from '@/store'
import { buildQuickLaunchAgentMenuEntries } from '@/components/tab-bar/QuickLaunchButton'
import { DEFAULT_DISABLED_TUI_AGENTS } from '../../../../shared/tui-agent-selection'
import type { TuiAgent } from '../../../../shared/tui-agent'
import { translate } from '@/i18n/i18n'

// Why: cap per-agent batch launches so a stray click on "+" cannot spawn an
// unbounded wall of PTYs; 10 matches the largest realistic parallel-agent sweep.
const MAX_TABS_PER_AGENT = 10

function getCatalogLabel(agent: TuiAgent): string {
  if (agent === 'kimi') {
    return translate('auto.components.tab.bar.QuickLaunchButton.kimiCodeCliLabel', 'Kimi Code CLI')
  }
  return getAgentCatalog().find((entry) => entry.id === agent)?.label ?? agent
}

/**
 * Centered launcher shown when a tab group has no tabs. Mirrors the tab-bar
 * "+" menu (agents + terminal/browser/markdown) but as a pick-what-to-open
 * surface, with a per-agent session count for batch launches.
 */
export default function TabGroupEmptyState({
  worktreeId,
  groupId,
  onNewTerminal,
  onNewBrowser,
  onNewMarkdown,
  onNewSimulator,
  onActivateTerminal
}: {
  worktreeId: string
  groupId: string
  onNewTerminal: () => void
  onNewBrowser: () => void
  onNewMarkdown: () => void
  onNewSimulator?: () => void
  onActivateTerminal: (terminalId: string) => void
}): React.JSX.Element {
  const agentDetectionTarget = useAgentDetectionTargetForWorktree(worktreeId)
  const { detectedIds } = useDetectedAgents(agentDetectionTarget)
  const defaultAgent = useAppStore((s) => s.settings?.defaultTuiAgent)
  const disabledAgents = useAppStore(
    (s) => s.settings?.disabledTuiAgents ?? DEFAULT_DISABLED_TUI_AGENTS
  )
  const openSettingsPage = useAppStore((s) => s.openSettingsPage)
  const openSettingsTarget = useAppStore((s) => s.openSettingsTarget)
  // Why: per-agent counts live only here — batch size is a per-launch decision, not a persisted preference.
  const [sessionCounts, setSessionCounts] = useState<Record<string, number>>({})

  const entries = useMemo(
    () =>
      detectedIds
        ? buildQuickLaunchAgentMenuEntries(defaultAgent, detectedIds, disabledAgents)
        : [],
    [detectedIds, defaultAgent, disabledAgents]
  )
  const launchEntries = entries.filter((entry) => entry.action === 'launch')
  const installEntries = entries.filter((entry) => entry.action === 'install')

  const openAgentSettings = useCallback(() => {
    openSettingsTarget({ pane: 'agents', repoId: null })
    openSettingsPage()
  }, [openSettingsPage, openSettingsTarget])


  const setSessionCount = useCallback((agent: TuiAgent, count: number) => {
    setSessionCounts((previous) => ({
      ...previous,
      [agent]: Math.min(MAX_TABS_PER_AGENT, Math.max(1, count))
    }))
  }, [])

  const runLaunch = useCallback(
    (agent: TuiAgent) => {
      const label = getCatalogLabel(agent)
      const count = sessionCounts[agent] ?? 1
      let lastTabId: string | null = null
      for (let index = 0; index < count; index += 1) {
        const result = launchAgentInNewTab({
          agent,
          worktreeId,
          groupId,
          launchSource: 'tab_bar_quick_launch'
        })
        if (!result) {
          toast.error(
            translate(
              'auto.components.tab.bar.QuickLaunchButton.465e432ef1',
              'Could not build launch command for {{value0}}.',
              { value0: label }
            )
          )
          break
        }
        if (result.tabId) {
          lastTabId = result.tabId
        }
      }
      // Why: focus the final tab of the batch so repeated launches land the user in the newest session, matching quick-launch behavior.
      if (lastTabId) {
        onActivateTerminal(lastTabId)
      }
    },
    [groupId, onActivateTerminal, sessionCounts, worktreeId]
  )
  const openInstallPage = useCallback((agent: TuiAgent) => {
    const homepageUrl = getAgentCatalog().find((entry) => entry.id === agent)?.homepageUrl
    if (!homepageUrl) {
      return
    }
    void window.api.shell.openUrl(homepageUrl).catch(() => {
      toast.error(
        translate(
          'auto.components.tab.group.TabGroupEmptyState.installPageFailed',
          'Could not open the {{value0}} installation page.',
          { value0: getCatalogLabel(agent) }
        )
      )
    })
  }, [])

  const surfaceTiles = useMemo<
    {
      id: string
      label: string
      icon: React.ComponentType<{ className?: string }>
      onClick: () => void
    }[]
  >(() => {
    const tiles: {
      id: string
      label: string
      icon: React.ComponentType<{ className?: string }>
      onClick: () => void
    }[] = [
      {
        id: 'terminal',
        label: translate('auto.components.tab.group.TabGroupEmptyState.newTerminal', 'Terminal'),
        icon: SquareTerminal,
        onClick: onNewTerminal
      },
      {
        id: 'browser',
        label: translate('auto.components.tab.group.TabGroupEmptyState.newBrowser', 'Browser'),
        icon: Globe,
        onClick: onNewBrowser
      },
      {
        id: 'markdown',
        label: translate('auto.components.tab.group.TabGroupEmptyState.newMarkdown', 'Markdown'),
        icon: FileText,
        onClick: onNewMarkdown
      }
    ]
    if (onNewSimulator) {
      tiles.push({
        id: 'simulator',
        label: translate('auto.components.tab.group.TabGroupEmptyState.newSimulator', 'Simulator'),
        icon: Smartphone,
        onClick: onNewSimulator
      })
    }
    return tiles
  }, [onNewBrowser, onNewMarkdown, onNewSimulator, onNewTerminal])

  return (
    <div
      className="absolute inset-0 flex items-center justify-center overflow-y-auto scrollbar-sleek p-6"
      data-tab-group-empty-state={groupId}
    >
      <div className="flex w-full max-w-[440px] flex-col gap-5 animate-in fade-in slide-in-from-bottom-3 duration-300 motion-reduce:animate-none">
        <div className="flex flex-col items-center gap-1.5 text-center">
          <div className="flex items-center gap-2">
            <span
              aria-hidden
              className="size-1.5 rounded-full bg-ai-action-accent shadow-[0_0_0_3px_color-mix(in_srgb,var(--ai-action-accent)_16%,transparent)]"
            />
            <h2 className="text-[15px] font-medium tracking-[-0.01em] text-foreground">
              {translate(
                'auto.components.tab.group.TabGroupEmptyState.3f9c81d2ae',
                'This pane is empty'
              )}
            </h2>
          </div>
          <p className="max-w-[38ch] text-xs leading-5 text-muted-foreground">
            {translate(
              'auto.components.tab.group.TabGroupEmptyState.b2d74e05c1',
              'Launch agents side by side, or open a terminal, browser, or note.'
            )}
          </p>
        </div>

        <div className="rounded-lg border border-border bg-card p-1.5 shadow-[0_10px_24px_rgb(0_0_0/0.18)]">
          {detectedIds === null ? (
            <div className="px-2.5 py-2.5 text-[12px] text-muted-foreground">
              {translate(
                'auto.components.tab.group.TabGroupEmptyState.detectingAgents',
                'Detecting agents…'
              )}
            </div>
          ) : launchEntries.length === 0 ? (
            <div className="px-2.5 py-2.5 text-[12px] text-muted-foreground">
              {detectedIds.length > 0
                ? translate(
                    'auto.components.tab.bar.QuickLaunchButton.8dea9b5cdf',
                    'No enabled agents'
                  )
                : translate(
                    'auto.components.tab.bar.QuickLaunchButton.e518f544b1',
                    'No agents detected'
                  )}
            </div>
          ) : (
            launchEntries.map(({ agent }) => {
              const count = sessionCounts[agent] ?? 1
              return (
                <div
                  key={agent}
                  role="button"
                  tabIndex={0}
                  data-agent-launch-row={agent}
                  onClick={() => {
                    runLaunch(agent)
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault()
                      runLaunch(agent)
                    }
                  }}
                  className="group flex h-11 cursor-pointer items-center gap-3 rounded-md px-2.5 outline-none transition-colors hover:bg-accent/40 focus-visible:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ai-action-accent"
                  title={translate(
                    'auto.components.tab.group.TabGroupEmptyState.launchAgentTooltip',
                    'Launch {{value0}} × {{value1}}',
                    { value0: getCatalogLabel(agent), value1: count }
                  )}
                >
                  <span
                    aria-hidden
                    data-agent-channel-led={agent}
                    className="size-1.5 shrink-0 rounded-full"
                    style={{ background: AGENT_CHANNEL_COLOR[agent] ?? 'var(--muted-foreground)' }}
                  />
                  <AgentIcon agent={agent} size={16} />
                  <span className="flex-1 truncate text-[13px] font-medium text-foreground">
                    {getCatalogLabel(agent)}
                  </span>
                  <ArrowRight className="size-3.5 shrink-0 text-ai-action-accent opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-visible:opacity-100" />
                  <div
                    role="group"
                    aria-label={translate(
                      'auto.components.tab.group.TabGroupEmptyState.sessionCountLabel',
                      'Sessions to open'
                    )}
                    className="flex items-center gap-0.5"
                    onClick={(event) => {
                      event.stopPropagation()
                    }}
                    onKeyDown={(event) => {
                      event.stopPropagation()
                    }}
                  >
                    <button
                      type="button"
                      aria-label={translate(
                        'auto.components.tab.group.TabGroupEmptyState.decrementSessions',
                        'Fewer {{value0}} sessions',
                        { value0: getCatalogLabel(agent) }
                      )}
                      disabled={count <= 1}
                      data-agent-count-decrement={agent}
                      onClick={() => {
                        setSessionCount(agent, count - 1)
                      }}
                      className="flex size-5 items-center justify-center rounded-md text-muted-foreground hover:bg-accent/60 hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
                    >
                      <Minus className="size-3" />
                    </button>
                    <span
                      data-agent-count={agent}
                      className={`readout w-7 text-center text-[11px] ${
                        count > 1 ? 'font-medium text-ai-action-accent' : 'text-muted-foreground'
                      }`}
                    >
                      {count}
                    </span>
                    <button
                      type="button"
                      aria-label={translate(
                        'auto.components.tab.group.TabGroupEmptyState.incrementSessions',
                        'More {{value0}} sessions',
                        { value0: getCatalogLabel(agent) }
                      )}
                      disabled={count >= MAX_TABS_PER_AGENT}
                      data-agent-count-increment={agent}
                      onClick={() => {
                        setSessionCount(agent, count + 1)
                      }}
                      className="flex size-5 items-center justify-center rounded-md text-muted-foreground hover:bg-accent/60 hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
                    >
                      <Plus className="size-3" />
                    </button>
                  </div>
                </div>
              )
            })
          )}
          {installEntries.length > 0 ? (
            <div className="mt-1 border-t border-border/70 pt-1">
              {installEntries.map(({ agent }) => (
                <button
                  key={agent}
                  type="button"
                  data-agent-install-row={agent}
                  onClick={() => {
                    openInstallPage(agent)
                  }}
                  className="flex h-9 w-full items-center gap-3 rounded-md px-2.5 text-left transition-colors hover:bg-accent/40"
                >
                  <AgentIcon agent={agent} size={16} />
                  <span className="flex-1 truncate text-[13px] font-medium text-muted-foreground">
                    {getCatalogLabel(agent)}
                  </span>
                  <span className="readout text-[10px] tracking-[0.08em] text-muted-foreground/85">
                    {translate('auto.components.settings.AgentsPane.f95b5c79b8', 'Install')}
                  </span>
                </button>
              ))}
            </div>
          ) : null}
        </div>

        <div className="flex divide-x divide-border overflow-hidden rounded-lg border border-border bg-card">
          {surfaceTiles.map(({ id, label, icon: Icon, onClick }) => (
            <button
              key={id}
              type="button"
              data-empty-state-tile={id}
              onClick={onClick}
              className="group flex h-16 flex-1 flex-col items-center justify-center gap-1.5 text-muted-foreground outline-none transition-colors hover:bg-accent/40 hover:text-foreground focus-visible:bg-accent/40 focus-visible:text-foreground focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ai-action-accent"
            >
              <Icon className="size-4 transition-transform duration-150 group-hover:-translate-y-0.5" />
              <span className="text-[11px] font-medium">{label}</span>
            </button>
          ))}
        </div>

        <button
          type="button"
          data-empty-state-agent-settings
          onClick={openAgentSettings}
          className="mx-auto flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] text-muted-foreground outline-none hover:bg-accent/50 hover:text-foreground focus-visible:bg-accent/50"
        >
          <Settings2 className="size-3" />
          {translate('auto.components.tab.bar.QuickLaunchButton.348a04c1ad', 'Agent settings…')}
        </button>
      </div>
    </div>
  )
}

// Test Bench channel identity: a stable accent hue per agent family, defined
// as theme-aware CSS variables in main.css (canonical map lives in
// right-sidebar/ai-vault-session-row-display.tsx; duplicated here to keep the
// tab-group chunk free of the right-sidebar module graph). Unknown agents fall
// back to muted foreground so unrecognized entries stay quiet.
const AGENT_CHANNEL_COLOR: Record<string, string> = {
  claude: 'var(--channel-claude)',
  'claude-agent-teams': 'var(--channel-claude)',
  openclaude: 'var(--channel-claude)',
  codex: 'var(--channel-codex)',
  grok: 'var(--channel-grok)',
  pi: 'var(--channel-pi)',
  omp: 'var(--channel-omp)'
}
