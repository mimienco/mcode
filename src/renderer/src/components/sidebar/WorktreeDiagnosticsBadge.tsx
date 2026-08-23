import type React from 'react'
import { CircleAlert } from 'lucide-react'

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { translate } from '@/i18n/i18n'
import { useAppStore } from '@/store'
import { useWorkspaceDiagnosticsStore } from '@/store/workspace-diagnostics'
import { workspaceDiagnosticsIdentity } from '@/store/workspace-diagnostics'
import { getExecutionHostIdForWorktree } from '@/lib/worktree-runtime-owner'
import type { ExecutionHostId } from '../../../../shared/execution-host'

export function WorktreeDiagnosticsBadge({
  worktreeId,
  hostId,
  onPointerDown
}: {
  worktreeId: string
  hostId?: ExecutionHostId
  onPointerDown?: (event: React.PointerEvent<HTMLButtonElement>) => void
}): React.JSX.Element | null {
  const resolvedHostId = useAppStore(
    (state) => hostId ?? getExecutionHostIdForWorktree(state, worktreeId)
  )
  const run = useWorkspaceDiagnosticsStore(
    (state) =>
      state.runsByWorktreeId[workspaceDiagnosticsIdentity(resolvedHostId, worktreeId)] ?? null
  )
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
  const label =
    failed > 0
      ? translate(
          'auto.components.sidebar.worktreeDiagnostics.failedChecks',
          '{{value0}} failed checks',
          { value0: failed }
        )
      : translate('auto.components.sidebar.worktreeDiagnostics.checkFailed', 'Check failed')
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex size-6 shrink-0 items-center justify-center rounded outline-none focus-visible:ring-1 focus-visible:ring-sidebar-ring"
          aria-label={label}
          onPointerDown={onPointerDown}
          onClick={() => {
            // Why microtask: card activation restores chrome first; Problems must win.
            queueMicrotask(onOpen)
          }}
        >
          <span className="inline-flex h-4 items-center gap-0.5 rounded border border-destructive/35 bg-destructive/10 px-1 text-[10px] font-semibold leading-none tabular-nums text-destructive transition-colors hover:bg-destructive/15">
            <CircleAlert className="size-2.5" aria-hidden="true" />
            {failed > 0 ? failed : '!'}
          </span>
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
