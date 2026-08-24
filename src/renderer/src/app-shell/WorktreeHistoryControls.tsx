import { ArrowLeft, ArrowRight } from 'lucide-react'
import { ShortcutKeyCombo } from '@/components/ShortcutKeyCombo'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { translate } from '@/i18n/i18n'
import { shouldShowWorktreeHistoryControls } from '@/lib/titlebar-worktree-history-controls'
import {
  canGoBackWorktreeHistory,
  canGoForwardWorktreeHistory
} from '@/store/slices/worktree-nav-history'
import { useShortcutKeyDetails } from '../hooks/useShortcutLabel'
import { useAppStore } from '../store'
import type { AppChromeLayout } from './use-app-chrome-layout'

function HistoryTooltip({
  label,
  shortcut
}: {
  label: string
  shortcut: ReturnType<typeof useShortcutKeyDetails>
}): React.JSX.Element {
  return (
    <span className="flex items-center gap-2">
      <span>{label}</span>
      {shortcut.keys.length > 0 ? (
        <ShortcutKeyCombo
          keys={shortcut.keys}
          doubleTap={shortcut.doubleTap}
          keyCapClassName="min-w-4 border-background/25 bg-background/10 px-1 py-0 text-[10px] text-background shadow-none"
          separatorClassName="text-[10px] text-background/70"
        />
      ) : null}
    </span>
  )
}

/** Global workspace/page history; unavailable directions stay out of the compact titlebar. */
export function WorktreeHistoryControls({
  activeView
}: {
  activeView: AppChromeLayout['activeView']
}): React.JSX.Element | null {
  const canGoBack = useAppStore(canGoBackWorktreeHistory)
  const canGoForward = useAppStore(canGoForwardWorktreeHistory)
  const backShortcut = useShortcutKeyDetails('worktree.history.back')
  const forwardShortcut = useShortcutKeyDetails('worktree.history.forward')
  const backLabel = translate('auto.App.064bd07810', 'Go back')
  const forwardLabel = translate('auto.App.cf9099fe98', 'Go forward')

  if (!shouldShowWorktreeHistoryControls(activeView) || (!canGoBack && !canGoForward)) {
    return null
  }

  return (
    <div className="titlebar-history-controls" data-testid="titlebar-history-controls">
      {canGoBack ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className="sidebar-toggle sidebar-toggle-compact"
              onClick={() => useAppStore.getState().goBackWorktree()}
              aria-label={backLabel}
            >
              <ArrowLeft size={13} />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom" sideOffset={6}>
            <HistoryTooltip label={backLabel} shortcut={backShortcut} />
          </TooltipContent>
        </Tooltip>
      ) : null}
      {canGoForward ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className="sidebar-toggle sidebar-toggle-compact"
              onClick={() => useAppStore.getState().goForwardWorktree()}
              aria-label={forwardLabel}
            >
              <ArrowRight size={13} />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom" sideOffset={6}>
            <HistoryTooltip label={forwardLabel} shortcut={forwardShortcut} />
          </TooltipContent>
        </Tooltip>
      ) : null}
    </div>
  )
}
