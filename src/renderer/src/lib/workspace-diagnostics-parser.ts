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

const VITEST_FAILURE = /^\s*FAIL\s+(.+?)\s+>\s+(.+)$/
const VITEST_FRAME = /^\s*[❯>]\s+(.+?):(\d+):(\d+)\s*$/
const VITEST_TESTS = /^\s*Tests\s+(.+)$/
const JEST_FAILURE_TITLE = /^\s*●\s+(.+)$/
const JEST_FRAME = /^\s*at\s+.*?\((.+?):(\d+):(\d+)\)\s*$/
const JEST_TESTS = /^\s*Tests:\s+(?:(\d+) failed,?\s*)?(?:(\d+) passed,?\s*)?(\d+) total/i
const PYTEST_FAILURE = /^FAILED\s+(.+?)::([^\s]+)\s+-\s+(.+)$/
const PYTEST_LOCATION = /^(.+?):(\d+):\s*(.+)$/
const PYTEST_SUMMARY = /={2,}\s*(?:(\d+) failed(?:,\s*)?)?(?:(\d+) passed)?\s+in\s+/i
const TYPESCRIPT_DIAGNOSTIC = /^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.+)$/i
const RUFF_DIAGNOSTIC = /^(.+?):(\d+):(\d+):\s+([A-Z]+\d+)\s+(?:\[\*\]\s+)?(.+)$/
const ESLINT_DIAGNOSTIC = /^\s+(\d+):(\d+)\s+(error|warning)\s+(.+?)\s{2,}(\S+)\s*$/i
const SOURCE_FILE_PATH = /\.[A-Za-z0-9_+-]+$/

function countSummaryValue(summary: string, label: string): number {
  const match = summary.match(new RegExp(`(?:^|\\|)\\s*(\\d+)\\s+${label}\\b`, 'i'))
  return match ? Number(match[1]) : 0
}

function diagnosticId(diagnostic: Omit<WorkspaceDiagnostic, 'id'>): string {
  return [
    diagnostic.kind,
    diagnostic.filePath,
    diagnostic.line,
    diagnostic.column ?? '',
    diagnostic.title,
    diagnostic.message
  ].join(':')
}

export function parseWorkspaceDiagnosticsOutput(
  output: string,
  exitCode: number | null
): WorkspaceDiagnosticsRun | null {
  const lines = stripAnsiEscapeSequences(output).replace(/\r\n?/g, '\n').split('\n')
  const pytestSummaryLine = lines.find(
    (line) => PYTEST_SUMMARY.test(line) && /\b(?:failed|passed)\b/i.test(line)
  )
  if (pytestSummaryLine) {
    const match = pytestSummaryLine.match(PYTEST_SUMMARY)!
    const diagnostics: WorkspaceDiagnostic[] = []
    for (const line of lines) {
      const failure = line.match(PYTEST_FAILURE)
      if (!failure) {
        continue
      }
      const filePath = failure[1].trim()
      const title = failure[2].trim()
      const location = lines
        .map((candidate) => candidate.match(PYTEST_LOCATION))
        .find((candidate) => candidate?.[1].trim() === filePath)
      const diagnostic = {
        kind: 'test' as const,
        severity: 'error' as const,
        filePath,
        line: Number(location?.[2] ?? 1),
        column: null,
        title,
        message: failure[3].trim(),
        code: null
      }
      diagnostics.push({ ...diagnostic, id: diagnosticId(diagnostic) })
    }
    const failed = Number(match[1] ?? 0)
    const passed = Number(match[2] ?? 0)
    return {
      tool: 'pytest',
      status: failed > 0 || (exitCode !== null && exitCode !== 0) ? 'failed' : 'passed',
      failed,
      passed,
      total: failed + passed,
      diagnostics
    }
  }
  const jestSummaryLine = lines.find((line) => JEST_TESTS.test(line))
  if (jestSummaryLine) {
    const match = jestSummaryLine.match(JEST_TESTS)!
    const diagnostics: WorkspaceDiagnostic[] = []
    let title = 'Test failed'
    let message = 'Test failed'
    for (const line of lines) {
      const titleMatch = line.match(JEST_FAILURE_TITLE)
      if (titleMatch) {
        title = titleMatch[1].trim()
        message = 'Test failed'
        continue
      }
      if (title !== 'Test failed' && /^\s{4}\S/.test(line) && !line.trimStart().startsWith('at ')) {
        message = line.trim()
        continue
      }
      const frame = line.match(JEST_FRAME)
      if (frame) {
        const diagnostic = {
          kind: 'test' as const,
          severity: 'error' as const,
          filePath: frame[1].trim(),
          line: Number(frame[2]),
          column: Number(frame[3]),
          title,
          message,
          code: null
        }
        diagnostics.push({ ...diagnostic, id: diagnosticId(diagnostic) })
      }
    }
    const failed = Number(match[1] ?? 0)
    const passed = Number(match[2] ?? 0)
    return {
      tool: 'jest',
      status: failed > 0 || (exitCode !== null && exitCode !== 0) ? 'failed' : 'passed',
      failed,
      passed,
      total: Number(match[3]),
      diagnostics
    }
  }
  const typescriptDiagnostics = lines.flatMap((line): WorkspaceDiagnostic[] => {
    const match = line.match(TYPESCRIPT_DIAGNOSTIC)
    if (!match) {
      return []
    }
    const diagnostic = {
      kind: 'compiler' as const,
      severity: match[4].toLowerCase() === 'warning' ? ('warning' as const) : ('error' as const),
      filePath: match[1].trim(),
      line: Number(match[2]),
      column: Number(match[3]),
      title: match[5],
      message: match[6].trim(),
      code: match[5]
    }
    return [{ ...diagnostic, id: diagnosticId(diagnostic) }]
  })
  if (typescriptDiagnostics.length > 0) {
    const failed = typescriptDiagnostics.filter((item) => item.severity === 'error').length
    return {
      tool: 'typescript',
      status: failed > 0 || (exitCode !== null && exitCode !== 0) ? 'failed' : 'passed',
      failed,
      passed: 0,
      total: typescriptDiagnostics.length,
      diagnostics: typescriptDiagnostics
    }
  }
  const ruffDiagnostics = lines.flatMap((line): WorkspaceDiagnostic[] => {
    const match = line.match(RUFF_DIAGNOSTIC)
    if (!match) {
      return []
    }
    const diagnostic = {
      kind: 'lint' as const,
      severity: 'error' as const,
      filePath: match[1].trim(),
      line: Number(match[2]),
      column: Number(match[3]),
      title: match[4],
      message: match[5].trim(),
      code: match[4]
    }
    return [{ ...diagnostic, id: diagnosticId(diagnostic) }]
  })
  if (ruffDiagnostics.length > 0) {
    return {
      tool: 'ruff',
      status: 'failed',
      failed: ruffDiagnostics.length,
      passed: 0,
      total: ruffDiagnostics.length,
      diagnostics: ruffDiagnostics
    }
  }
  const eslintDiagnostics: WorkspaceDiagnostic[] = []
  let eslintFilePath = ''
  for (const line of lines) {
    const trimmed = line.trim()
    if (trimmed && SOURCE_FILE_PATH.test(trimmed) && !/^\d/.test(trimmed)) {
      eslintFilePath = trimmed
      continue
    }
    const match = line.match(ESLINT_DIAGNOSTIC)
    if (!match || !eslintFilePath) {
      continue
    }
    const diagnostic = {
      kind: 'lint' as const,
      severity: match[3].toLowerCase() === 'warning' ? ('warning' as const) : ('error' as const),
      filePath: eslintFilePath,
      line: Number(match[1]),
      column: Number(match[2]),
      title: match[5],
      message: match[4].trim(),
      code: match[5]
    }
    eslintDiagnostics.push({ ...diagnostic, id: diagnosticId(diagnostic) })
  }
  if (eslintDiagnostics.length > 0) {
    const failed = eslintDiagnostics.filter((item) => item.severity === 'error').length
    return {
      tool: 'eslint',
      status: failed > 0 || (exitCode !== null && exitCode !== 0) ? 'failed' : 'passed',
      failed,
      passed: 0,
      total: eslintDiagnostics.length,
      diagnostics: eslintDiagnostics
    }
  }
  const diagnostics: WorkspaceDiagnostic[] = []
  let currentFailure: { filePath: string; title: string; message: string } | null = null
  let summary: { failed: number; passed: number } | null = null

  for (const line of lines) {
    const failure = line.match(VITEST_FAILURE)
    if (failure) {
      currentFailure = { filePath: failure[1].trim(), title: failure[2].trim(), message: '' }
      continue
    }
    if (currentFailure && /^\s*(?:AssertionError|Error):\s*/.test(line)) {
      currentFailure.message = line.replace(/^\s*(?:AssertionError|Error):\s*/, '').trim()
      continue
    }
    const frame = line.match(VITEST_FRAME)
    if (currentFailure && frame) {
      const diagnostic = {
        kind: 'test' as const,
        severity: 'error' as const,
        filePath: frame[1].trim(),
        line: Number(frame[2]),
        column: Number(frame[3]),
        title: currentFailure.title,
        message: currentFailure.message || 'Test failed',
        code: null
      }
      diagnostics.push({ ...diagnostic, id: diagnosticId(diagnostic) })
      currentFailure = null
      continue
    }
    const tests = line.match(VITEST_TESTS)
    if (tests) {
      summary = {
        failed: countSummaryValue(tests[1], 'failed'),
        passed: countSummaryValue(tests[1], 'passed')
      }
    }
  }

  if (!summary && diagnostics.length === 0) {
    return null
  }
  const failed = summary?.failed ?? diagnostics.length
  const passed = summary?.passed ?? 0
  return {
    tool: 'vitest',
    status: failed > 0 || (exitCode !== null && exitCode !== 0) ? 'failed' : 'passed',
    failed,
    passed,
    total: failed + passed,
    diagnostics
  }
}
