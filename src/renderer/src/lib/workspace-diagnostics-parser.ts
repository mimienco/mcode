import {
  parseEslintDiagnostics,
  parseRuffDiagnostics,
  parseTypeScriptDiagnostics
} from './workspace-check-diagnostics-parser'
import {
  cleanDiagnosticsOutput,
  type WorkspaceDiagnosticsRun,
  type WorkspaceDiagnosticTool
} from './workspace-diagnostics-parser-core'
import {
  parseJestDiagnostics,
  parsePytestDiagnostics,
  parseVitestDiagnostics
} from './workspace-test-diagnostics-parser'

export type {
  WorkspaceDiagnostic,
  WorkspaceDiagnosticsRun,
  WorkspaceDiagnosticTool
} from './workspace-diagnostics-parser-core'

export function detectWorkspaceDiagnosticTool(command: string): WorkspaceDiagnosticTool | null {
  const clean = cleanDiagnosticsOutput(command).trim()
  if (/\bvitest\b/i.test(clean)) {
    return 'vitest'
  }
  if (/\bjest\b/i.test(clean)) {
    return 'jest'
  }
  if (/\bpytest\b/i.test(clean)) {
    return 'pytest'
  }
  if (/\b(?:tsc|typescript)\b/i.test(clean)) {
    return 'typescript'
  }
  if (/\beslint\b/i.test(clean)) {
    return 'eslint'
  }
  if (/\bruff\b/i.test(clean)) {
    return 'ruff'
  }
  if (/\b(?:pnpm|npm(?:\s+run)?|yarn|bun)\s+(?:run\s+)?typecheck\b/i.test(clean)) {
    return 'typescript'
  }
  if (/\b(?:pnpm|npm(?:\s+run)?|yarn|bun)\s+(?:run\s+)?lint\b/i.test(clean)) {
    return 'eslint'
  }
  return null
}

export function parseWorkspaceDiagnosticsOutput(
  output: string,
  exitCode: number | null
): WorkspaceDiagnosticsRun | null {
  return parseWorkspaceDiagnosticsRunsOutput(output, exitCode)[0] ?? null
}

export function parseWorkspaceDiagnosticsRunsOutput(
  output: string,
  exitCode: number | null
): WorkspaceDiagnosticsRun[] {
  const lines = cleanDiagnosticsOutput(output).split('\n')
  return [
    parsePytestDiagnostics(lines, exitCode),
    parseJestDiagnostics(lines, exitCode),
    parseTypeScriptDiagnostics(lines, exitCode),
    parseRuffDiagnostics(lines),
    parseEslintDiagnostics(lines, exitCode),
    parseVitestDiagnostics(lines, exitCode)
  ].filter((run): run is WorkspaceDiagnosticsRun => run !== null)
}
