import {
  createWorkspaceDiagnostic,
  type WorkspaceDiagnostic,
  type WorkspaceDiagnosticsRun
} from './workspace-diagnostics-parser-core'

const TYPESCRIPT_DIAGNOSTIC_PAREN = /^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.+)$/i
const TYPESCRIPT_DIAGNOSTIC_PRETTY = /^(.+?):(\d+):(\d+)\s+-\s+(error|warning)\s+(TS\d+):\s+(.+)$/i
const RUFF_DIAGNOSTIC = /^(.+?):(\d+):(\d+):\s+([A-Z]+\d+)\s+(?:\[\*\]\s+)?(.+)$/
const RUFF_FULL_HEADER = /^([A-Z]+\d+)\s+(?:\[\*\]\s+)?(.+)$/
const RUFF_FULL_LOCATION = /^\s*-->\s+(.+?):(\d+):(\d+)\s*$/
const ESLINT_DIAGNOSTIC = /^\s+(\d+):(\d+)\s+(error|warning)\s+(.+?)\s{2,}(\S+)\s*$/i
const SOURCE_FILE_PATH = /\.[A-Za-z0-9_+-]+$/

export function parseTypeScriptDiagnostics(
  lines: readonly string[],
  _exitCode: number | null
): WorkspaceDiagnosticsRun | null {
  const diagnostics = lines.flatMap((line): WorkspaceDiagnostic[] => {
    const match =
      line.match(TYPESCRIPT_DIAGNOSTIC_PAREN) ?? line.match(TYPESCRIPT_DIAGNOSTIC_PRETTY)
    if (!match) {
      return []
    }
    return [
      createWorkspaceDiagnostic({
        kind: 'compiler',
        severity: match[4].toLowerCase() === 'warning' ? 'warning' : 'error',
        filePath: match[1].trim(),
        line: Number(match[2]),
        column: Number(match[3]),
        title: match[5],
        message: match[6].trim(),
        code: match[5]
      })
    ]
  })
  if (diagnostics.length === 0) {
    return null
  }
  const failed = diagnostics.filter((item) => item.severity === 'error').length
  return {
    tool: 'typescript',
    status: failed > 0 ? 'failed' : 'passed',
    failed,
    passed: 0,
    total: diagnostics.length,
    diagnostics
  }
}

export function parseRuffDiagnostics(lines: readonly string[]): WorkspaceDiagnosticsRun | null {
  const diagnostics: WorkspaceDiagnostic[] = []
  let fullHeader: RegExpMatchArray | null = null
  for (const line of lines) {
    const concise = line.match(RUFF_DIAGNOSTIC)
    if (concise) {
      diagnostics.push(
        createWorkspaceDiagnostic({
          kind: 'lint',
          severity: 'error',
          filePath: concise[1].trim(),
          line: Number(concise[2]),
          column: Number(concise[3]),
          title: concise[4],
          message: concise[5].trim(),
          code: concise[4]
        })
      )
      fullHeader = null
      continue
    }
    const header = line.match(RUFF_FULL_HEADER)
    if (header) {
      fullHeader = header
      continue
    }
    const location = line.match(RUFF_FULL_LOCATION)
    if (fullHeader && location) {
      diagnostics.push(
        createWorkspaceDiagnostic({
          kind: 'lint',
          severity: 'error',
          filePath: location[1].trim(),
          line: Number(location[2]),
          column: Number(location[3]),
          title: fullHeader[1],
          message: fullHeader[2].trim(),
          code: fullHeader[1]
        })
      )
      fullHeader = null
    }
  }
  return diagnostics.length > 0
    ? {
        tool: 'ruff',
        status: 'failed',
        failed: diagnostics.length,
        passed: 0,
        total: diagnostics.length,
        diagnostics
      }
    : null
}

export function parseEslintDiagnostics(
  lines: readonly string[],
  _exitCode: number | null
): WorkspaceDiagnosticsRun | null {
  const diagnostics: WorkspaceDiagnostic[] = []
  let filePath = ''
  for (const line of lines) {
    const trimmed = line.trim()
    if (trimmed && SOURCE_FILE_PATH.test(trimmed) && !/^\d/.test(trimmed)) {
      filePath = trimmed
      continue
    }
    const match = line.match(ESLINT_DIAGNOSTIC)
    if (!match || !filePath) {
      continue
    }
    diagnostics.push(
      createWorkspaceDiagnostic({
        kind: 'lint',
        severity: match[3].toLowerCase() === 'warning' ? 'warning' : 'error',
        filePath,
        line: Number(match[1]),
        column: Number(match[2]),
        title: match[5],
        message: match[4].trim(),
        code: match[5]
      })
    )
  }
  if (diagnostics.length === 0) {
    return null
  }
  const failed = diagnostics.filter((item) => item.severity === 'error').length
  return {
    tool: 'eslint',
    status: failed > 0 ? 'failed' : 'passed',
    failed,
    passed: 0,
    total: diagnostics.length,
    diagnostics
  }
}
