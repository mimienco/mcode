import React, { useCallback, useMemo, useState } from 'react'
import { ArrowRight, FileText, Globe, Settings2, Smartphone, SquareTerminal } from 'lucide-react'
import { toast } from 'sonner'
import { getAgentCatalog } from '@/lib/agent-catalog'
import { launchAgentInNewTab } from '@/lib/launch-agent-in-new-tab'
import { useAgentDetectionTargetForWorktree } from '@/hooks/useAgentDetectionTarget'
import { useDetectedAgents } from '@/hooks/useDetectedAgents'
import { useAppStore } from '@/store'
import { buildQuickLaunchAgentMenuEntries } from '@/components/tab-bar/QuickLaunchButton'
import { Button } from '@/components/ui/button'
import { DEFAULT_DISABLED_TUI_AGENTS } from '../../../../shared/tui-agent-selection'
import type { TuiAgent } from '../../../../shared/tui-agent'
import { translate } from '@/i18n/i18n'
import { getEmptyStateAgentLabel } from './tab-group-empty-state-agent-metadata'
import { TabGroupEmptyStateAgentGrid } from './TabGroupEmptyStateAgentGrid'
import { TabGroupEmptyStateInstallList } from './TabGroupEmptyStateInstallList'
import { TabGroupEmptyStateTemplateRail } from './TabGroupEmptyStateTemplateRail'
import type { TabGroupWorkspaceTemplateId } from './tab-group-workspace-template'
import './tab-group-empty-state.css'

const MAX_TABS_PER_AGENT = 10

type SurfaceTile = {
  id: string
  label: string
  icon: React.ComponentType<{ className?: string }>
  onClick: () => void
}

export default function TabGroupEmptyState({
  worktreeId,
  groupId,
  onNewTerminal,
  onNewBrowser,
  onNewMarkdown,
  onNewSimulator,
  onUseTemplate,
  onActivateTerminal
}: {
  worktreeId: string
  groupId: string
  onNewTerminal: () => void
  onNewBrowser: () => void
  onNewMarkdown: () => void
  onNewSimulator?: () => void
  onUseTemplate: (templateId: TabGroupWorkspaceTemplateId, agent: TuiAgent) => Promise<void> | void
  onActivateTerminal: (terminalId: string) => void
}): React.JSX.Element {
  const agentDetectionTarget = useAgentDetectionTargetForWorktree(worktreeId)
  const { detectedIds } = useDetectedAgents(agentDetectionTarget)
  const defaultAgent = useAppStore((state) => state.settings?.defaultTuiAgent)
  const disabledAgents = useAppStore(
    (state) => state.settings?.disabledTuiAgents ?? DEFAULT_DISABLED_TUI_AGENTS
  )
  const openSettingsPage = useAppStore((state) => state.openSettingsPage)
  const openSettingsTarget = useAppStore((state) => state.openSettingsTarget)
  const [sessionCounts, setSessionCounts] = useState<Record<string, number>>({})
  const [selectedTemplateId, setSelectedTemplateId] =
    useState<TabGroupWorkspaceTemplateId>('research')
  const [isStartingTemplate, setIsStartingTemplate] = useState(false)

  const entries = useMemo(
    () =>
      detectedIds
        ? buildQuickLaunchAgentMenuEntries(defaultAgent, detectedIds, disabledAgents)
        : [],
    [detectedIds, defaultAgent, disabledAgents]
  )
  const launchEntries = entries.filter((entry) => entry.action === 'launch')
  const installEntries = entries.filter((entry) => entry.action === 'install')
  const templateAgent = launchEntries[0]?.agent ?? null
  const selectedSessionCount = launchEntries.reduce(
    (total, entry) => total + (sessionCounts[entry.agent] ?? 0),
    0
  )

  const openAgentSettings = useCallback(() => {
    openSettingsTarget({ pane: 'agents', repoId: null })
    openSettingsPage()
  }, [openSettingsPage, openSettingsTarget])

  const setSessionCount = useCallback((agent: TuiAgent, count: number) => {
    setSessionCounts((previous) => ({
      ...previous,
      [agent]: Math.min(MAX_TABS_PER_AGENT, Math.max(0, count))
    }))
  }, [])

  const launchSelectedAgents = useCallback(() => {
    let lastTabId: string | null = null
    for (const { agent } of launchEntries) {
      const count = sessionCounts[agent] ?? 0
      if (count === 0) {
        continue
      }
      const label = getEmptyStateAgentLabel(agent)
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
    }
    if (lastTabId) {
      onActivateTerminal(lastTabId)
    }
  }, [groupId, launchEntries, onActivateTerminal, sessionCounts, worktreeId])

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
          { value0: getEmptyStateAgentLabel(agent) }
        )
      )
    })
  }, [])

  const surfaceTiles = useMemo<SurfaceTile[]>(() => {
    const tiles: SurfaceTile[] = [
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

  const startTemplate = useCallback(
    async (templateId: TabGroupWorkspaceTemplateId) => {
      if (!templateAgent || isStartingTemplate) {
        return
      }
      setSelectedTemplateId(templateId)
      setIsStartingTemplate(true)
      try {
        await onUseTemplate(templateId, templateAgent)
      } finally {
        setIsStartingTemplate(false)
      }
    },
    [isStartingTemplate, onUseTemplate, templateAgent]
  )

  return (
    <div
      className="@container/empty-state absolute inset-0 flex overflow-x-hidden overflow-y-auto scrollbar-sleek p-2 @min-[360px]/empty-state:p-3 @min-[720px]/empty-state:p-5"
      data-tab-group-empty-state={groupId}
    >
      <div className="m-auto grid w-full max-w-[1280px] gap-3 animate-in fade-in slide-in-from-bottom-3 duration-300 motion-reduce:animate-none @min-[860px]/empty-state:grid-cols-[minmax(310px,0.36fr)_minmax(0,1fr)]">
        <TabGroupEmptyStateTemplateRail
          selectedTemplateId={selectedTemplateId}
          isStartingTemplate={isStartingTemplate}
          onSelectTemplate={setSelectedTemplateId}
          onUseTemplate={(templateId) => {
            void startTemplate(templateId)
          }}
        />

        <main className="flex min-w-0 flex-col gap-5 rounded-lg border border-border bg-background p-2.5 @min-[360px]/empty-state:p-4 @min-[720px]/empty-state:p-5 @min-[860px]/empty-state:p-6">
          <div className="space-y-1 @min-[860px]/empty-state:space-y-2">
            <h1 className="text-xl font-semibold text-foreground @min-[860px]/empty-state:text-3xl">
              {translate(
                'auto.components.tab.group.TabGroupEmptyState.workspaceHeading',
                'Build your workspace'
              )}
            </h1>
            <p className="text-xs leading-5 text-muted-foreground @min-[860px]/empty-state:text-base">
              {translate(
                'auto.components.tab.group.TabGroupEmptyState.workspaceDescription',
                'Choose agents and tools to work side by side.'
              )}
            </p>
          </div>

          <section
            className="space-y-2.5 @min-[860px]/empty-state:space-y-4"
            aria-labelledby={`${groupId}-agent-heading`}
          >
            <h2
              id={`${groupId}-agent-heading`}
              className="text-[11px] font-semibold uppercase tracking-[0.05em] text-muted-foreground @min-[860px]/empty-state:text-sm"
            >
              {translate('auto.components.tab.group.TabGroupEmptyState.agentsHeading', 'Agents')}
            </h2>
            {detectedIds === null ? (
              <div className="rounded-lg border border-border bg-card px-3 py-4 text-xs text-muted-foreground">
                {translate(
                  'auto.components.tab.group.TabGroupEmptyState.detectingAgents',
                  'Detecting agents…'
                )}
              </div>
            ) : (
              <>
                {launchEntries.length === 0 ? (
                  <div className="rounded-lg border border-border bg-card px-3 py-4 text-xs text-muted-foreground">
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
                ) : null}
                <TabGroupEmptyStateAgentGrid
                  launchEntries={launchEntries}
                  sessionCounts={sessionCounts}
                  maxSessions={MAX_TABS_PER_AGENT}
                  onSetSessionCount={setSessionCount}
                />
              </>
            )}
          </section>

          <section
            className="space-y-2.5 @min-[860px]/empty-state:space-y-4"
            aria-labelledby={`${groupId}-tool-heading`}
          >
            <h2
              id={`${groupId}-tool-heading`}
              className="text-[11px] font-semibold uppercase tracking-[0.05em] text-muted-foreground @min-[860px]/empty-state:text-sm"
            >
              {translate('auto.components.tab.group.TabGroupEmptyState.toolsHeading', 'Tools')}
            </h2>
            <div className="grid grid-cols-2 gap-2 @min-[680px]/empty-state:grid-cols-4">
              {surfaceTiles.map(({ id, label, icon: Icon, onClick }) => (
                <Button
                  key={id}
                  type="button"
                  variant="outline"
                  data-empty-state-tile={id}
                  onClick={onClick}
                  className="group h-20 flex-col gap-2 whitespace-normal bg-card text-muted-foreground outline-none transition-colors hover:border-muted-foreground/35 hover:bg-accent/35 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ai-action-accent @min-[860px]/empty-state:h-28 @min-[860px]/empty-state:gap-3"
                >
                  <Icon className="size-5 text-ai-action-accent @min-[860px]/empty-state:size-8" />
                  <span className="text-xs font-medium @min-[860px]/empty-state:text-base @min-[860px]/empty-state:text-foreground">
                    {label}
                  </span>
                </Button>
              ))}
            </div>
          </section>

          <TabGroupEmptyStateInstallList
            installEntries={installEntries}
            onInstall={openInstallPage}
          />

          <footer className="mt-auto flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4 @min-[860px]/empty-state:pt-6">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-empty-state-agent-settings
              onClick={openAgentSettings}
              className="px-2 text-xs text-muted-foreground @min-[860px]/empty-state:h-10 @min-[860px]/empty-state:text-sm"
            >
              <Settings2 />
              {translate('auto.components.tab.bar.QuickLaunchButton.348a04c1ad', 'Agent settings…')}
            </Button>
            <Button
              type="button"
              data-start-workspace
              disabled={selectedSessionCount === 0 || isStartingTemplate}
              onClick={launchSelectedAgents}
              className="min-w-36 @min-[860px]/empty-state:h-12 @min-[860px]/empty-state:min-w-48 @min-[860px]/empty-state:text-base"
            >
              {translate(
                'auto.components.tab.group.TabGroupEmptyState.startWorkspace',
                'Start workspace'
              )}
              <span className="readout text-xs opacity-75">{selectedSessionCount}</span>
              <ArrowRight />
            </Button>
          </footer>
        </main>
      </div>
    </div>
  )
}
