import type React from 'react'
import { CircleAlert } from 'lucide-react'

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { translate } from '@/i18n/i18n'
import { useAppStore } from '@/store'
import { useWorkspaceDiagnosticsStore } from '@/store/workspace-diagnostics'

export function WorktreeDiagnosticsBadge({
  worktreeId,
  onPointerDown
}: {
  worktreeId: string
  onPointerDown?: (event: React.PointerEvent<HTMLButtonElement>) => void
}): React.JSX.Element | null {
  const run = useWorkspaceDiagnosticsStore((state) => state.runsByWorktreeId[worktreeId] ?? null)
  const setRightSidebarOpen = useAppStore((state) => state.setRightSidebarOpen)
  const setRightSidebarTab = useAppStore((state) => state.setRightSidebarTab)
  if (!run || run.status !== 'failed') {
    return null
  }

  return (
    <WorktreeDiagnosticsBadgeView
      failed={run.failed}
      onPointerDown={onPointerDown}
      onOpen={() => {
        setRightSidebarOpen(true)
        setRightSidebarTab('problems')
      }}
    />
  )
}

export function WorktreeDiagnosticsBadgeView({
  failed,
  onOpen,
  onPointerDown
}: {
  failed: number
  onOpen: () => void
  onPointerDown?: (event: React.PointerEvent<HTMLButtonElement>) => void
}): React.JSX.Element {
  const label = translate(
    'auto.components.sidebar.worktreeDiagnostics.failedChecks',
    '{{value0}} failed checks',
    { value0: failed }
  )
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex h-4 shrink-0 items-center gap-0.5 rounded border border-destructive/35 bg-destructive/10 px-1 text-[10px] font-semibold leading-none tabular-nums text-destructive outline-none transition-colors hover:bg-destructive/15 focus-visible:ring-1 focus-visible:ring-sidebar-ring"
          aria-label={label}
          onPointerDown={onPointerDown}
          onClick={() => {
            // Why microtask: card activation restores chrome first; Problems must win.
            queueMicrotask(onOpen)
          }}
        >
          <CircleAlert className="size-2.5" aria-hidden="true" />
          {failed}
        </button>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={8}>
        {translate(
          'auto.components.sidebar.worktreeDiagnostics.openProblems',
          '{{value0}}. Open Problems.',
          { value0: label }
        )}
      </TooltipContent>
    </Tooltip>
  )
}
