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

export function detectWorkspaceDiagnosticTool(output: string): WorkspaceDiagnosticTool | null {
  const clean = cleanDiagnosticsOutput(output)
  if (/\bvitest\b/i.test(clean) || /^\s*Tests\s+.+\|/m.test(clean)) {
    return 'vitest'
  }
  if (/\bjest\b/i.test(clean) || /^\s*Tests:\s+.+\btotal\b/im.test(clean)) {
    return 'jest'
  }
  if (/\bpytest\b/i.test(clean) || /^=+\s+.+\s+in\s+[\d.]+s?\s*=*$/im.test(clean)) {
    return 'pytest'
  }
  if (/\b(?:tsc|typescript)\b/i.test(clean) || /\bTS\d+:/.test(clean)) {
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
  const lines = cleanDiagnosticsOutput(output).split('\n')
  return (
    parsePytestDiagnostics(lines, exitCode) ??
    parseJestDiagnostics(lines, exitCode) ??
    parseTypeScriptDiagnostics(lines, exitCode) ??
    parseRuffDiagnostics(lines) ??
    parseEslintDiagnostics(lines, exitCode) ??
    parseVitestDiagnostics(lines, exitCode)
  )
}
