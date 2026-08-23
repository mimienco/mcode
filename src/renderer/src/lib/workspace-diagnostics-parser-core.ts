import { stripAnsiEscapeSequences } from '../../../shared/ansi-escape-sequences'

export type WorkspaceDiagnosticTool =
  | 'vitest'
  | 'jest'
  | 'pytest'
  | 'typescript'
  | 'eslint'
  | 'ruff'
  | 'unknown'

export type WorkspaceDiagnostic = {
  id: string
  kind: 'test' | 'compiler' | 'lint'
  severity: 'error' | 'warning'
  filePath: string
  /** Shell directory of the command that emitted a relative file path. */
  basePath?: string
  line: number
  column: number | null
  title: string
  message: string
  code: string | null
}

export type WorkspaceDiagnosticsRun = {
  tool: WorkspaceDiagnosticTool
  status: 'passed' | 'failed'
  failed: number
  passed: number
  total: number
  diagnostics: WorkspaceDiagnostic[]
}

export function cleanDiagnosticsOutput(output: string): string {
  return stripAnsiEscapeSequences(output).replace(/\r\n?/g, '\n')
}

export function countDiagnosticsSummaryValue(summary: string, label: string): number {
  const match = summary.match(new RegExp(`(?:^|[,|])\\s*(\\d+)\\s+${label}\\b`, 'i'))
  return match ? Number(match[1]) : 0
}

export function createWorkspaceDiagnostic(
  diagnostic: Omit<WorkspaceDiagnostic, 'id'>
): WorkspaceDiagnostic {
  const id = [
    diagnostic.kind,
    diagnostic.filePath,
    diagnostic.line,
    diagnostic.column ?? '',
    diagnostic.title,
    diagnostic.message
  ].join(':')
  return { ...diagnostic, id }
}
