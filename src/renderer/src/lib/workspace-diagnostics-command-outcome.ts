import type { WorkspaceDiagnosticCommandAnalysis } from './workspace-diagnostics-parser'
import type {
  WorkspaceDiagnosticsRun,
  WorkspaceDiagnosticTool
} from './workspace-diagnostics-parser-core'

function synthesizeRun(
  tool: WorkspaceDiagnosticTool,
  exitCode: number | null
): WorkspaceDiagnosticsRun {
  const failed = exitCode !== null && exitCode !== 0
  return {
    tool,
    status: failed ? 'failed' : 'passed',
    failed: failed ? 1 : 0,
    passed: 0,
    total: failed ? 1 : 0,
    diagnostics: []
  }
}

function runRepresentsTool(run: WorkspaceDiagnosticsRun, tool: WorkspaceDiagnosticTool): boolean {
  return tool === 'unknown' ? ['vitest', 'jest', 'pytest'].includes(run.tool) : run.tool === tool
}

export function resolveWorkspaceDiagnosticCommandRuns(args: {
  commandAnalysis: WorkspaceDiagnosticCommandAnalysis | null
  exitCode: number | null
  parsedRuns: WorkspaceDiagnosticsRun[]
}): WorkspaceDiagnosticsRun[] {
  const { commandAnalysis, exitCode } = args
  const toolHint = commandAnalysis?.tool ?? null
  let parsedRuns = args.parsedRuns
  const representedRunIndex = toolHint
    ? parsedRuns.findIndex((run) => runRepresentsTool(run, toolHint))
    : -1
  const selectedStepIsTerminal =
    commandAnalysis !== null &&
    commandAnalysis.selectedStepIndex === commandAnalysis.steps.length - 1
  const priorStep = commandAnalysis?.steps[commandAnalysis.selectedStepIndex - 1]
  const selectedStepExecutionIsProven =
    commandAnalysis !== null &&
    (commandAnalysis.precedingOperator !== 'and' ||
      (priorStep?.tool !== null &&
        priorStep?.tool !== undefined &&
        parsedRuns.some((run) => run.tool === priorStep.tool && run.status === 'passed')))
  const selectedToolOwnsExitCode =
    selectedStepIsTerminal && (representedRunIndex !== -1 || selectedStepExecutionIsProven)

  if (
    exitCode !== null &&
    exitCode !== 0 &&
    selectedToolOwnsExitCode &&
    representedRunIndex !== -1 &&
    parsedRuns[representedRunIndex]?.status === 'passed'
  ) {
    parsedRuns = parsedRuns.map((run, index) =>
      index === representedRunIndex ? { ...run, status: 'failed', failed: 1 } : run
    )
  }

  const unresolvedToolRun =
    toolHint &&
    exitCode !== null &&
    exitCode !== 0 &&
    representedRunIndex === -1 &&
    selectedToolOwnsExitCode
      ? synthesizeRun(toolHint, exitCode)
      : null
  const terminalStep = commandAnalysis?.steps.at(-1)
  const terminalStepWasSkippedAfterFailure =
    terminalStep?.precedingOperator === 'and' && parsedRuns.some((run) => run.status === 'failed')
  const selectedToolAccountsForExit =
    selectedToolOwnsExitCode && (representedRunIndex !== -1 || unresolvedToolRun !== null)
  const unresolvedCommandRun =
    commandAnalysis &&
    exitCode !== null &&
    exitCode !== 0 &&
    !selectedToolAccountsForExit &&
    !terminalStepWasSkippedAfterFailure
      ? synthesizeRun('command', exitCode)
      : null

  if (parsedRuns.length > 0) {
    return [
      ...parsedRuns,
      ...(unresolvedToolRun ? [unresolvedToolRun] : []),
      ...(unresolvedCommandRun ? [unresolvedCommandRun] : [])
    ]
  }
  if (!toolHint) {
    return []
  }
  if (unresolvedCommandRun) {
    return [unresolvedCommandRun]
  }
  return [unresolvedToolRun ?? synthesizeRun(toolHint, exitCode)]
}
