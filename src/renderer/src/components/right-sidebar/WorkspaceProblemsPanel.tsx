import { useCallback } from 'react'
import { AlertCircle, CheckCircle2, CircleAlert, Trash2 } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import { getLocalProjectExecutionRuntimeContext } from '@/lib/local-preflight-context'
import { joinPath } from '@/lib/path'
import { resolveTerminalFileLinkText } from '@/lib/terminal-links'
import {
  getExecutionHostIdForWorktree,
  getRuntimeEnvironmentIdForWorktree
} from '@/lib/worktree-runtime-owner'
import { useAppStore } from '@/store'
import { useActiveWorktree } from '@/store/selectors'
import {
  useWorkspaceDiagnosticsStore,
  workspaceDiagnosticsIdentity,
  type WorkspaceDiagnosticsSummary
} from '@/store/workspace-diagnostics'
import type { WorkspaceDiagnostic } from '@/lib/workspace-diagnostics-parser'
import { openDetectedFilePath } from '../terminal-pane/terminal-file-open-routing'
import { activateTabAndFocusPane } from '@/lib/activate-tab-and-focus-pane'
import { resolveTerminalTabPtyOwnership } from '@/lib/terminal-tab-for-pty-id'
import {
  locationlessFailedOutputs,
  showWorkspaceOutputLabel,
  workspaceDiagnosticToolLabel,
  workspaceDiagnosticsLiveStatus,
  workspaceDiagnosticsStatusLabel,
  workspaceDiagnosticsWarningLabel
} from './workspace-problems-panel-model'

const NO_OUTPUT_PTYS = new Set<string>()

function groupDiagnosticsByFile(
  diagnostics: readonly WorkspaceDiagnostic[]
): [string, WorkspaceDiagnostic[]][] {
  const groups = new Map<string, WorkspaceDiagnostic[]>()
  for (const diagnostic of diagnostics) {
    const group = groups.get(diagnostic.filePath)
    if (group) {
      group.push(diagnostic)
    } else {
      groups.set(diagnostic.filePath, [diagnostic])
    }
  }
  return Array.from(groups.entries())
}

function diagnosticLocationLabel(diagnostic: WorkspaceDiagnostic): string {
  return diagnostic.column !== null
    ? translate(
        'auto.components.right.sidebar.problems.lineColumnLocation',
        'Line {{value0}}:{{value1}}',
        { value0: diagnostic.line, value1: diagnostic.column }
      )
    : translate('auto.components.right.sidebar.problems.lineLocation', 'Line {{value0}}', {
        value0: diagnostic.line
      })
}

export function WorkspaceDiagnosticsList({
  run,
  onClear,
  onOpenDiagnostic,
  onShowOutput,
  availableOutputPtyIds = NO_OUTPUT_PTYS
}: {
  run: WorkspaceDiagnosticsSummary
  onClear: () => void
  onOpenDiagnostic: (diagnostic: WorkspaceDiagnostic) => void
  onShowOutput?: (ptyId: string) => void
  availableOutputPtyIds?: ReadonlySet<string>
}): React.JSX.Element {
  const warningCount = run.diagnostics.filter((item) => item.severity === 'warning').length
  const groups = groupDiagnosticsByFile(run.diagnostics)
  const locationlessOutputs = locationlessFailedOutputs(run)
  const availableLocationlessOutputs = locationlessOutputs.filter(({ ptyId }) =>
    availableOutputPtyIds.has(ptyId)
  )
  const StatusIcon = run.status === 'passed' ? CheckCircle2 : AlertCircle

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-11 items-center gap-2 border-b border-border px-3">
        <StatusIcon
          className={
            run.status === 'passed' ? 'size-4 text-status-success' : 'size-4 text-destructive'
          }
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-xs">
            <span className="font-semibold text-foreground">
              {workspaceDiagnosticsStatusLabel(run)}
            </span>
            {run.passed > 0 ? (
              <span className="text-muted-foreground">
                {translate(
                  'auto.components.right.sidebar.problems.passedCount',
                  '{{value0}} passed',
                  {
                    value0: run.passed
                  }
                )}
              </span>
            ) : null}
            {warningCount > 0 ? (
              <span className="text-muted-foreground">
                {workspaceDiagnosticsWarningLabel(warningCount)}
              </span>
            ) : null}
          </div>
          <div className="truncate text-[11px] text-muted-foreground">
            {run.tools.map(workspaceDiagnosticToolLabel).join(' · ')}
          </div>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onClear}
          aria-label={translate(
            'auto.components.right.sidebar.problems.clear',
            'Clear diagnostics'
          )}
        >
          <Trash2 className="size-3.5" />
        </Button>
      </div>

      {groups.length === 0 ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
          {run.status === 'failed' ? (
            <AlertCircle className="mb-3 size-7 text-destructive" aria-hidden="true" />
          ) : (
            <CheckCircle2 className="mb-3 size-7 text-status-success" aria-hidden="true" />
          )}
          <p className="text-sm font-medium text-foreground">
            {run.status === 'failed'
              ? translate('auto.components.right.sidebar.problems.checkFailed', 'Check failed')
              : run.total > 0
                ? translate(
                    'auto.components.right.sidebar.problems.allPassed',
                    'All {{value0}} tests passed',
                    { value0: run.total }
                  )
                : translate('auto.components.right.sidebar.problems.healthy', 'No problems found')}
          </p>
          <p className="mt-1 max-w-64 text-xs leading-relaxed text-muted-foreground">
            {run.status === 'failed'
              ? translate(
                  'auto.components.right.sidebar.problems.noSourceLocation',
                  'The check exited unsuccessfully without a source location.'
                )
              : translate(
                  'auto.components.right.sidebar.problems.latestRunHealthy',
                  'The latest observed check completed without errors.'
                )}
          </p>
          {onShowOutput
            ? availableLocationlessOutputs.map(({ ptyId, tool }) => (
                <Button
                  key={ptyId}
                  type="button"
                  variant="outline"
                  size="sm"
                  className="mt-3"
                  onClick={() => onShowOutput(ptyId)}
                >
                  {showWorkspaceOutputLabel(tool)}
                </Button>
              ))
            : null}
        </div>
      ) : (
        <div className="scrollbar-sleek min-h-0 flex-1 overflow-y-auto py-1">
          {locationlessOutputs.length > 0 ? (
            <div className="mx-2 mb-1 rounded border border-destructive/25 bg-destructive/5 px-2.5 py-2">
              <p className="text-[11px] leading-relaxed text-muted-foreground">
                {translate(
                  'auto.components.right.sidebar.problems.noSourceLocation',
                  'The check exited unsuccessfully without a source location.'
                )}
              </p>
              {onShowOutput && availableLocationlessOutputs.length > 0 ? (
                <div className="mt-1.5 flex flex-wrap gap-1.5">
                  {availableLocationlessOutputs.map(({ ptyId, tool }) => (
                    <Button
                      key={ptyId}
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => onShowOutput(ptyId)}
                    >
                      {showWorkspaceOutputLabel(tool)}
                    </Button>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}
          {groups.map(([filePath, diagnostics]) => (
            <section key={filePath} aria-label={filePath}>
              <div className="sticky top-0 z-[1] flex items-center gap-2 border-y border-border/70 bg-sidebar px-3 py-1.5 text-[11px] font-medium text-muted-foreground first:border-t-0">
                <span className="min-w-0 flex-1 truncate font-mono">{filePath}</span>
                <span className="tabular-nums">{diagnostics.length}</span>
              </div>
              {diagnostics.map((diagnostic) => (
                <button
                  key={diagnostic.id}
                  type="button"
                  className="group flex w-full items-start gap-2 px-3 py-2 text-left outline-none transition-colors hover:bg-sidebar-accent focus-visible:bg-sidebar-accent focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-sidebar-ring"
                  onClick={() => onOpenDiagnostic(diagnostic)}
                  aria-label={`${diagnostic.title}. ${diagnostic.message}. ${filePath}, ${diagnosticLocationLabel(diagnostic)}`}
                >
                  {diagnostic.severity === 'warning' ? (
                    <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-500" />
                  ) : (
                    <AlertCircle className="mt-0.5 size-3.5 shrink-0 text-destructive" />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-xs font-medium text-foreground">
                      {diagnostic.title}
                    </span>
                    <span className="mt-0.5 block text-[11px] leading-relaxed text-muted-foreground [overflow-wrap:anywhere]">
                      {diagnostic.message}
                    </span>
                    <span className="mt-1 block font-mono text-[10px] text-muted-foreground/80 group-hover:text-foreground/70">
                      {diagnosticLocationLabel(diagnostic)}
                    </span>
                  </span>
                </button>
              ))}
            </section>
          ))}
        </div>
      )}
    </div>
  )
}

function resolveDiagnosticFilePath(filePath: string, worktreePath: string): string {
  const resolved = resolveTerminalFileLinkText(filePath, worktreePath)
  if (resolved) {
    return resolved.absolutePath
  }
  if (/^(?:[A-Za-z]:[\\/]|[/\\]{2}|\/)/.test(filePath)) {
    return filePath
  }
  return joinPath(worktreePath, filePath)
}

export default function WorkspaceProblemsPanel(): React.JSX.Element {
  const worktree = useActiveWorktree()
  const activeHostId = useAppStore((state) => getExecutionHostIdForWorktree(state, worktree?.id))
  const workspaceIdentity = worktree
    ? workspaceDiagnosticsIdentity(worktree.hostId ?? activeHostId, worktree.id)
    : null
  const run = useWorkspaceDiagnosticsStore((state) =>
    workspaceIdentity ? (state.runsByWorktreeId[workspaceIdentity] ?? null) : null
  )
  const clearWorktree = useWorkspaceDiagnosticsStore((state) => state.clearWorktree)
  const outputPtyIds = run ? locationlessFailedOutputs(run).map(({ ptyId }) => ptyId) : []
  const availableOutputPtyIdsKey = useAppStore((state) => {
    if (!worktree) {
      return ''
    }
    return outputPtyIds
      .filter((ptyId) => resolveTerminalTabPtyOwnership(state, worktree.id, ptyId).kind === 'owned')
      .join('\u0000')
  })
  const availableOutputPtyIds = new Set(
    availableOutputPtyIdsKey ? availableOutputPtyIdsKey.split('\u0000') : []
  )

  const openDiagnostic = useCallback(
    (diagnostic: WorkspaceDiagnostic): void => {
      if (!worktree) {
        return
      }
      const state = useAppStore.getState()
      const runtimeEnvironmentId = getRuntimeEnvironmentIdForWorktree(state, worktree.id)
      const localRuntime = getLocalProjectExecutionRuntimeContext(state, worktree.id)
      const wslDistro =
        localRuntime?.status === 'resolved' && localRuntime.runtime.kind === 'wsl'
          ? localRuntime.runtime.distro
          : undefined
      openDetectedFilePath(
        resolveDiagnosticFilePath(diagnostic.filePath, diagnostic.basePath ?? worktree.path),
        diagnostic.line,
        diagnostic.column,
        {
          worktreeId: worktree.id,
          worktreePath: worktree.path,
          runtimeEnvironmentId,
          wslDistro
        }
      )
    },
    [worktree]
  )

  const showOutput = useCallback(
    (ptyId: string): void => {
      if (!worktree) {
        return
      }
      const state = useAppStore.getState()
      const ownership = resolveTerminalTabPtyOwnership(state, worktree.id, ptyId)
      if (ownership.kind !== 'owned') {
        return
      }
      const layout = state.terminalLayoutsByTabId[ownership.tabId]
      const leafId = Object.entries(layout?.ptyIdsByLeafId ?? {}).find(
        ([, candidatePtyId]) => candidatePtyId === ptyId
      )?.[0]
      state.setRightSidebarOpen(false)
      activateTabAndFocusPane(ownership.tabId, leafId ?? layout?.activeLeafId ?? null, {
        flashFocusedPane: true,
        scrollToBottomIfOutputSinceLastView: true
      })
    },
    [worktree]
  )

  const liveStatus = run ? workspaceDiagnosticsLiveStatus(run) : ''
  const liveRegion = (
    <span className="sr-only" aria-live="polite" aria-atomic="true">
      {liveStatus}
    </span>
  )

  if (!worktree || !run) {
    return (
      <>
        {liveRegion}
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
          <CircleAlert className="mb-3 size-7 text-muted-foreground/60" aria-hidden="true" />
          <p className="text-sm font-medium text-foreground">
            {translate('auto.components.right.sidebar.problems.emptyTitle', 'No diagnostics yet')}
          </p>
          <p className="mt-1 max-w-64 text-xs leading-relaxed text-muted-foreground">
            {translate(
              'auto.components.right.sidebar.problems.emptyDescription',
              'Run tests, typecheck, or lint in a terminal. Failures will appear here automatically.'
            )}
          </p>
        </div>
      </>
    )
  }

  return (
    <>
      {liveRegion}
      <WorkspaceDiagnosticsList
        run={run}
        onClear={() => clearWorktree(worktree.id, worktree.hostId ?? activeHostId)}
        onOpenDiagnostic={openDiagnostic}
        onShowOutput={showOutput}
        availableOutputPtyIds={availableOutputPtyIds}
      />
    </>
  )
}
