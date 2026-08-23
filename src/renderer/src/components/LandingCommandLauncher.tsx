import { ArrowRight, FolderPlus, GitBranchPlus, Loader2, MessageSquarePlus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ShortcutKeyCombo } from '@/components/ShortcutKeyCombo'
import type { ShortcutKeyComboDetails } from '@/hooks/useShortcutLabel'
import { cn } from '@/lib/utils'

export type LandingCommandLauncherProps = {
  quickChatAvailable: boolean
  creatingQuickChat: boolean
  newChatLabel: string
  addProjectLabel: string
  createTargetActionLabel: string
  createTargetShortcut: ShortcutKeyComboDetails
  previousWorkspaceShortcut: ShortcutKeyComboDetails
  nextWorkspaceShortcut: ShortcutKeyComboDetails
  onCreateQuickChat: () => void
  onCreateTarget: () => void
  onAddProject: () => void
}

type ShortcutHintProps = {
  label: string
  shortcut: ShortcutKeyComboDetails
  className?: string
}

function ShortcutHint({ label, shortcut, className }: ShortcutHintProps): React.JSX.Element {
  return (
    <div className={cn('flex min-w-0 items-center gap-1.5', className)}>
      <span className="whitespace-nowrap text-xs text-muted-foreground">{label}</span>
      <ShortcutKeyCombo
        keys={shortcut.keys}
        doubleTap={shortcut.doubleTap}
        className="shrink-0"
        separatorClassName="mx-0 text-[10px] text-muted-foreground"
        keyCapClassName="bg-muted/70 text-[11px] shadow-none"
      />
    </div>
  )
}

export function LandingCommandLauncher({
  quickChatAvailable,
  creatingQuickChat,
  newChatLabel,
  addProjectLabel,
  createTargetActionLabel,
  createTargetShortcut,
  previousWorkspaceShortcut,
  nextWorkspaceShortcut,
  onCreateQuickChat,
  onCreateTarget,
  onAddProject
}: LandingCommandLauncherProps): React.JSX.Element {
  const primaryLabel = quickChatAvailable ? newChatLabel : createTargetActionLabel
  const primaryShortcut = quickChatAvailable ? null : createTargetShortcut
  const primaryAction = quickChatAvailable ? onCreateQuickChat : onCreateTarget
  const primaryDisabled = quickChatAvailable && creatingQuickChat

  return (
    <div className="w-full max-w-lg text-left">
      <Button
        type="button"
        data-landing-command="primary"
        className="group h-14 w-full justify-start gap-0 overflow-hidden rounded-lg bg-ai-action-accent p-0 text-base text-primary-foreground shadow-xs hover:bg-ai-action-accent/90"
        disabled={primaryDisabled}
        aria-busy={primaryDisabled}
        onClick={primaryAction}
      >
        <span className="flex h-full w-14 shrink-0 items-center justify-center border-r border-primary-foreground/20">
          {creatingQuickChat && quickChatAvailable ? (
            <Loader2 className="size-5 animate-spin" />
          ) : quickChatAvailable ? (
            <MessageSquarePlus className="size-5" />
          ) : (
            <GitBranchPlus className="size-5" />
          )}
        </span>
        <span className="px-5 font-semibold">{primaryLabel}</span>
        {primaryShortcut ? (
          <ShortcutKeyCombo
            keys={primaryShortcut.keys}
            doubleTap={primaryShortcut.doubleTap}
            className="ml-auto shrink-0"
            separatorClassName="mx-0 text-[10px] text-primary-foreground/70"
            keyCapClassName="border-primary-foreground/25 bg-primary-foreground/10 text-[11px] text-primary-foreground shadow-none"
          />
        ) : null}
        <ArrowRight className="mr-5 ml-auto size-5 transition-transform duration-200 ease-out group-hover:translate-x-1" />
      </Button>

      <div
        className={cn(
          'relative mt-4 grid grid-cols-1 gap-2',
          quickChatAvailable && 'sm:mt-6 sm:grid-cols-2'
        )}
      >
        {quickChatAvailable ? (
          <svg
            aria-hidden
            data-landing-connectors
            className="pointer-events-none absolute -top-6 left-0 h-6 w-full overflow-visible text-ai-action-accent max-sm:hidden"
            viewBox="0 0 512 24"
            preserveAspectRatio="none"
          >
            <path
              d="M28 0V12H128V24M28 12H384V24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            <circle cx="28" cy="12" r="2" fill="currentColor" />
            <circle cx="128" cy="24" r="2" fill="currentColor" />
            <circle cx="384" cy="24" r="2" fill="currentColor" />
          </svg>
        ) : null}
        {quickChatAvailable ? (
          <Button
            type="button"
            variant="outline"
            data-landing-command="secondary"
            className="h-12 min-w-0 justify-start gap-0 overflow-hidden rounded-lg bg-card p-0 shadow-xs hover:border-muted-foreground/35 hover:bg-accent"
            onClick={onCreateTarget}
          >
            <span className="flex h-full w-12 shrink-0 items-center justify-center">
              <GitBranchPlus className="size-5" />
            </span>
            <span className="min-w-0 truncate pr-2 font-semibold">{createTargetActionLabel}</span>
            <ShortcutKeyCombo
              keys={createTargetShortcut.keys}
              doubleTap={createTargetShortcut.doubleTap}
              className="ml-auto mr-3 shrink-0"
              separatorClassName="mx-0 text-[10px] text-muted-foreground"
              keyCapClassName="bg-muted/70 text-[11px] shadow-none"
            />
          </Button>
        ) : null}

        <Button
          type="button"
          variant="outline"
          data-landing-command="secondary"
          className="h-12 min-w-0 justify-start gap-0 overflow-hidden rounded-lg bg-card p-0 shadow-xs hover:border-muted-foreground/35 hover:bg-accent"
          onClick={onAddProject}
        >
          <span className="flex h-full w-12 shrink-0 items-center justify-center">
            <FolderPlus className="size-5" />
          </span>
          <span className="min-w-0 truncate pr-3 font-semibold">{addProjectLabel}</span>
        </Button>
      </div>

      <div className="mt-3 grid grid-cols-1 gap-2 sm:grid-cols-2">
        <ShortcutHint
          label="Move up workspace"
          shortcut={previousWorkspaceShortcut}
          className="justify-center sm:justify-start"
        />
        <ShortcutHint
          label="Move down workspace"
          shortcut={nextWorkspaceShortcut}
          className="justify-center sm:justify-end"
        />
      </div>
    </div>
  )
}
