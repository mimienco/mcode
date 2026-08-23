import { useCallback } from 'react'
import { AlertCircle, CheckCircle2, CircleAlert, Trash2 } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import { getLocalProjectExecutionRuntimeContext } from '@/lib/local-preflight-context'
import { joinPath } from '@/lib/path'
import { resolveTerminalFileLinkText } from '@/lib/terminal-links'
import { getRuntimeEnvironmentIdForWorktree } from '@/lib/worktree-runtime-owner'
import { useAppStore } from '@/store'
import { useActiveWorktree } from '@/store/selectors'
import {
  useWorkspaceDiagnosticsStore,
  type WorkspaceDiagnosticsEntry
} from '@/store/workspace-diagnostics'
import type { WorkspaceDiagnostic } from '@/lib/workspace-diagnostics-parser'
import { openDetectedFilePath } from '../terminal-pane/terminal-file-open-routing'

function toolLabel(tool: WorkspaceDiagnosticsEntry['tool']): string {
  const labels: Record<WorkspaceDiagnosticsEntry['tool'], string> = {
    vitest: 'Vitest',
    jest: 'Jest',
    pytest: 'pytest',
    typescript: 'TypeScript',
    eslint: 'ESLint',
    ruff: 'Ruff',
    unknown: translate('auto.components.right.sidebar.problems.unknownTool', 'Diagnostics')
  }
  return labels[tool]
}

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

export function WorkspaceDiagnosticsList({
  run,
  onClear,
  onOpenDiagnostic
}: {
  run: WorkspaceDiagnosticsEntry
  onClear: () => void
  onOpenDiagnostic: (diagnostic: WorkspaceDiagnostic) => void
}): React.JSX.Element {
  const warningCount = run.diagnostics.filter((item) => item.severity === 'warning').length
  const groups = groupDiagnosticsByFile(run.diagnostics)
  const StatusIcon = run.status === 'passed' ? CheckCircle2 : AlertCircle

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-11 items-center gap-2 border-b border-border px-3">
        <StatusIcon
          className={run.status === 'passed' ? 'size-4 text-emerald-500' : 'size-4 text-destructive'}
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2 text-xs">
            <span className="font-semibold text-foreground">
              {run.failed > 0
                ? translate('auto.components.right.sidebar.problems.failedCount', '{{value0}} failed', {
                    value0: run.failed
                  })
                : run.status === 'failed'
                  ? translate(
                      'auto.components.right.sidebar.problems.checkFailed',
                      'Check failed'
                    )
                : translate('auto.components.right.sidebar.problems.noFailures', 'No failures')}
            </span>
            {run.passed > 0 ? (
              <span className="text-muted-foreground">
                {translate('auto.components.right.sidebar.problems.passedCount', '{{value0}} passed', {
                  value0: run.passed
                })}
              </span>
            ) : null}
            {warningCount > 0 ? (
              <span className="text-muted-foreground">
                {translate('auto.components.right.sidebar.problems.warningCount', '{{value0}} warnings', {
                  value0: warningCount
                })}
              </span>
            ) : null}
          </div>
          <div className="text-[11px] text-muted-foreground">{toolLabel(run.tool)}</div>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onClear}
          aria-label={translate('auto.components.right.sidebar.problems.clear', 'Clear diagnostics')}
        >
          <Trash2 className="size-3.5" />
        </Button>
      </div>

      {groups.length === 0 ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
          {run.status === 'failed' ? (
            <AlertCircle className="mb-3 size-7 text-destructive" aria-hidden="true" />
          ) : (
            <CheckCircle2 className="mb-3 size-7 text-emerald-500" aria-hidden="true" />
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
        </div>
      ) : (
        <div className="scrollbar-sleek min-h-0 flex-1 overflow-y-auto py-1">
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
                  aria-label={`${diagnostic.title}, ${filePath}, ${translate(
                    'auto.components.right.sidebar.problems.lineLocation',
                    'Line {{value0}}',
                    { value0: diagnostic.line }
                  )}`}
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
                    <span className="mt-0.5 block text-[11px] leading-relaxed text-muted-foreground">
                      {diagnostic.message}
                    </span>
                    <span className="mt-1 block font-mono text-[10px] text-muted-foreground/80 group-hover:text-foreground/70">
                      {diagnostic.column
                        ? translate(
                            'auto.components.right.sidebar.problems.lineColumnLocation',
                            'Line {{value0}}:{{value1}}',
                            { value0: diagnostic.line, value1: diagnostic.column }
                          )
                        : translate(
                            'auto.components.right.sidebar.problems.lineLocation',
                            'Line {{value0}}',
                            { value0: diagnostic.line }
                          )}
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
  const run = useWorkspaceDiagnosticsStore((state) =>
    worktree ? (state.runsByWorktreeId[worktree.id] ?? null) : null
  )
  const clearWorktree = useWorkspaceDiagnosticsStore((state) => state.clearWorktree)

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
        resolveDiagnosticFilePath(diagnostic.filePath, worktree.path),
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

  if (!worktree || !run) {
    return (
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
    )
  }

  return (
    <WorkspaceDiagnosticsList
      run={run}
      onClear={() => clearWorktree(worktree.id)}
      onOpenDiagnostic={openDiagnostic}
    />
  )
}
