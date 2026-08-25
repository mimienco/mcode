import React from 'react'
import { ArrowRight, Check, Code2, Search, ShieldCheck } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import type { TabGroupWorkspaceTemplateId } from './tab-group-workspace-template'

type TemplateDefinition = {
  id: TabGroupWorkspaceTemplateId
  icon: React.ComponentType<{ className?: string; style?: React.CSSProperties }>
  label: string
  description: string
  layout: string
  accent: string
  preview: 'browser' | 'terminal' | 'review'
}

function templateDefinitions(): TemplateDefinition[] {
  return [
    {
      id: 'research',
      icon: Search,
      label: translate('auto.components.tab.group.TabGroupEmptyState.templateResearch', 'Research'),
      description: translate(
        'auto.components.tab.group.TabGroupEmptyState.templateResearchDescription',
        'Explore, gather, and synthesize information from the web.'
      ),
      layout: translate(
        'auto.components.tab.group.TabGroupEmptyState.templateResearchLayout',
        'Browser + agent'
      ),
      accent: 'var(--ai-action-accent)',
      preview: 'browser'
    },
    {
      id: 'build',
      icon: Code2,
      label: translate('auto.components.tab.group.TabGroupEmptyState.templateBuild', 'Build'),
      description: translate(
        'auto.components.tab.group.TabGroupEmptyState.templateBuildDescription',
        'Plan, code, and test with developer tools.'
      ),
      layout: translate(
        'auto.components.tab.group.TabGroupEmptyState.templateBuildLayout',
        'Agent + terminal'
      ),
      accent: 'var(--ai-action-accent)',
      preview: 'terminal'
    },
    {
      id: 'review',
      icon: ShieldCheck,
      label: translate('auto.components.tab.group.TabGroupEmptyState.templateReview', 'Review'),
      description: translate(
        'auto.components.tab.group.TabGroupEmptyState.templateReviewDescription',
        'Review changes, run checks, and refine output.'
      ),
      layout: translate(
        'auto.components.tab.group.TabGroupEmptyState.templateReviewLayout',
        'Markdown + agent'
      ),
      accent: 'var(--ai-action-accent)',
      preview: 'review'
    }
  ]
}

function TemplatePreview({ variant }: { variant: TemplateDefinition['preview'] }) {
  const channelColors = [
    'var(--channel-claude)',
    'var(--channel-codex)',
    'var(--channel-grok)',
    'var(--channel-pi)',
    'var(--muted-foreground)'
  ]

  return (
    <div
      aria-hidden
      className="tab-group-empty-state-template-preview h-[88px] grid-cols-[0.34fr_1fr] overflow-hidden rounded-md border border-border bg-background/70"
    >
      <div className="flex flex-col justify-center gap-2 border-r border-border bg-muted/30 p-2.5">
        {channelColors.map((color) => (
          <span key={color} className="flex items-center gap-1.5">
            <span className="size-1.5 shrink-0 rounded-full" style={{ background: color }} />
            <span className="h-1 flex-1 rounded-full bg-muted-foreground/35" />
          </span>
        ))}
      </div>
      <div className="grid grid-cols-[1.08fr_0.92fr] gap-1.5 p-2">
        <div className="flex min-w-0 flex-col rounded-sm border border-border bg-card p-2">
          <span className="mb-2 h-2 w-4/5 rounded-sm bg-muted-foreground/30" />
          {variant === 'review' ? (
            <div className="flex flex-1 flex-col justify-center gap-2">
              {[0, 1, 2].map((item) => (
                <span key={item} className="flex items-center gap-1.5">
                  <span className="size-1.5 rounded-full border border-muted-foreground/60" />
                  <span className="h-1 flex-1 rounded-full bg-muted-foreground/30" />
                </span>
              ))}
            </div>
          ) : (
            <span className="flex flex-1 items-center justify-center readout text-sm text-muted-foreground">
              {variant === 'terminal' ? '›_' : '•••'}
            </span>
          )}
        </div>
        <div className="flex items-center justify-center rounded-sm border border-border bg-card">
          {variant === 'browser' ? (
            <span className="size-6 rounded-full border-2 border-muted-foreground/55" />
          ) : variant === 'terminal' ? (
            <span className="h-8 w-4 rounded-sm border-2 border-muted-foreground/55" />
          ) : (
            <ShieldCheck className="size-7 text-muted-foreground/65" />
          )}
        </div>
      </div>
    </div>
  )
}

function templateCardStyle(accent: string, selected: boolean): React.CSSProperties {
  return {
    borderColor: `color-mix(in srgb, ${accent} ${selected ? '92%' : '58%'}, var(--border))`,
    background: `color-mix(in srgb, ${accent} ${selected ? '7%' : '2%'}, var(--card))`
  }
}

export function TabGroupEmptyStateTemplateRail({
  selectedTemplateId,
  isStartingTemplate,
  onSelectTemplate,
  onUseTemplate
}: {
  selectedTemplateId: TabGroupWorkspaceTemplateId
  isStartingTemplate: boolean
  onSelectTemplate: (templateId: TabGroupWorkspaceTemplateId) => void
  onUseTemplate: (templateId: TabGroupWorkspaceTemplateId) => void
}): React.JSX.Element {
  return (
    <aside className="relative flex min-w-0 flex-col gap-4 overflow-hidden rounded-lg border border-border bg-card/35 p-4 @min-[860px]/empty-state:gap-5 @min-[860px]/empty-state:p-5">
      <div
        aria-hidden
        className="tab-group-empty-state-template-dots pointer-events-none absolute inset-0 opacity-35 [background-image:radial-gradient(circle,var(--border)_1px,transparent_1px)] [background-size:24px_24px]"
      />
      <div className="relative space-y-1 @min-[860px]/empty-state:space-y-2">
        <h2 className="text-base font-semibold text-foreground @min-[860px]/empty-state:text-xl">
          {translate(
            'auto.components.tab.group.TabGroupEmptyState.templateHeading',
            'Start with a template'
          )}
        </h2>
        <p className="text-xs leading-5 text-muted-foreground @min-[860px]/empty-state:text-sm">
          {translate(
            'auto.components.tab.group.TabGroupEmptyState.templateDescription',
            'Choose a setup or build your own.'
          )}
        </p>
      </div>

      <div className="relative grid grid-cols-1 gap-2 @min-[520px]/empty-state:grid-cols-3 @min-[860px]/empty-state:grid-cols-1 @min-[860px]/empty-state:gap-4">
        {templateDefinitions().map(
          ({ id, icon: Icon, label, description, layout, accent, preview }) => {
            const selected = id === selectedTemplateId
            return (
              <article
                key={id}
                style={templateCardStyle(accent, selected)}
                className="group min-w-0 rounded-lg border p-1.5 transition-colors hover:bg-accent/20 @min-[860px]/empty-state:p-2"
              >
                <Button
                  type="button"
                  variant="ghost"
                  aria-pressed={selected}
                  data-workspace-template={id}
                  onClick={() => {
                    onSelectTemplate(id)
                  }}
                  className="h-auto w-full min-w-0 flex-col items-stretch justify-start gap-2 whitespace-normal p-1 text-left hover:bg-transparent focus-visible:ring-2 focus-visible:ring-ai-action-accent"
                >
                  <TemplatePreview variant={preview} />
                  <span className="flex min-w-0 items-center gap-2 @min-[860px]/empty-state:gap-3">
                    <Icon
                      className="size-4 shrink-0 @min-[860px]/empty-state:size-5"
                      style={{ color: accent }}
                    />
                    <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-foreground @min-[860px]/empty-state:text-base">
                      {label}
                    </span>
                    {selected ? <Check className="size-4 shrink-0 text-ai-action-accent" /> : null}
                  </span>
                  <span className="tab-group-empty-state-template-description text-xs leading-4 text-muted-foreground @min-[860px]/empty-state:text-sm @min-[860px]/empty-state:leading-5">
                    {description}
                  </span>
                  <span className="text-[11px] font-medium text-foreground/80 @min-[860px]/empty-state:text-xs">
                    {layout}
                  </span>
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-use-workspace-template={id}
                  disabled={isStartingTemplate}
                  onClick={() => {
                    onSelectTemplate(id)
                    onUseTemplate(id)
                  }}
                  className="mt-1 w-full justify-between px-2 text-ai-action-accent hover:bg-ai-action-accent/10 hover:text-ai-action-accent"
                >
                  {translate(
                    'auto.components.tab.group.TabGroupEmptyState.useTemplate',
                    'Use template'
                  )}
                  <ArrowRight />
                </Button>
              </article>
            )
          }
        )}
      </div>
    </aside>
  )
}
