import { translate } from '@/i18n/i18n'
import type { WorkspaceDiagnosticTool } from '@/lib/workspace-diagnostics-parser'
import type { WorkspaceDiagnosticsSummary } from '@/store/workspace-diagnostics'

export function workspaceDiagnosticToolLabel(tool: WorkspaceDiagnosticTool): string {
  const labels: Record<WorkspaceDiagnosticTool, string> = {
    vitest: 'Vitest',
    jest: 'Jest',
    pytest: 'pytest',
    typescript: 'TypeScript',
    eslint: 'ESLint',
    ruff: 'Ruff',
    command: translate('auto.components.right.sidebar.problems.commandTool', 'Command'),
    unknown: translate('auto.components.right.sidebar.problems.unknownTool', 'Diagnostics')
  }
  return labels[tool]
}

export function workspaceDiagnosticsStatusLabel(run: WorkspaceDiagnosticsSummary): string {
  if (run.failed > 0) {
    return translate('auto.components.right.sidebar.problems.failedCount', '{{value0}} failed', {
      value0: run.failed
    })
  }
  return run.status === 'failed'
    ? translate('auto.components.right.sidebar.problems.checkFailed', 'Check failed')
    : translate('auto.components.right.sidebar.problems.noFailures', 'No failures')
}

export function locationlessFailedOutputs(
  run: WorkspaceDiagnosticsSummary
): { ptyId: string; tool: WorkspaceDiagnosticTool | null }[] {
  const outputs = new Map<string, WorkspaceDiagnosticTool | null>()
  for (const entry of Object.values(run.runsByTool)) {
    const locatedErrors = entry?.diagnostics.filter(
      (diagnostic) => diagnostic.severity === 'error'
    ).length
    if (entry?.status === 'failed' && entry.failed > (locatedErrors ?? 0)) {
      outputs.set(entry.ptyId, entry.tool)
    }
  }
  if (
    outputs.size === 0 &&
    run.status === 'failed' &&
    run.diagnostics.length === 0 &&
    run.outputPtyId
  ) {
    outputs.set(run.outputPtyId, null)
  }
  return [...outputs].map(([ptyId, tool]) => ({ ptyId, tool }))
}

export function showWorkspaceOutputLabel(tool: WorkspaceDiagnosticTool | null): string {
  return tool
    ? translate('auto.components.right.sidebar.problems.showToolOutput', 'Show {{value0}} output', {
        value0: workspaceDiagnosticToolLabel(tool)
      })
    : translate('auto.components.right.sidebar.problems.showOutput', 'Show output')
}

export function workspaceDiagnosticsWarningLabel(warningCount: number): string {
  return warningCount === 1
    ? translate('auto.components.right.sidebar.problems.warning', '{{value0}} warning', {
        value0: warningCount
      })
    : translate('auto.components.right.sidebar.problems.warningCount', '{{value0}} warnings', {
        value0: warningCount
      })
}

export function workspaceDiagnosticsLiveStatus(run: WorkspaceDiagnosticsSummary): string {
  const warningCount = run.diagnostics.filter((item) => item.severity === 'warning').length
  return [
    workspaceDiagnosticsStatusLabel(run),
    run.passed > 0
      ? translate('auto.components.right.sidebar.problems.passedCount', '{{value0}} passed', {
          value0: run.passed
        })
      : null,
    warningCount > 0 ? workspaceDiagnosticsWarningLabel(warningCount) : null
  ]
    .filter((part): part is string => part !== null)
    .join('. ')
}
