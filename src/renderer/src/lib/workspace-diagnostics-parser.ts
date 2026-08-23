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

function commandTokens(segment: string): string[] {
  return (
    segment
      .match(/"(?:\\.|[^"])*"|'(?:\\.|[^'])*'|[^\s]+/g)
      ?.map((token) => token.replace(/^(?:"|')|(?:"|')$/g, '')) ?? []
  )
}

function commandName(token: string): string {
  return token
    .replace(/\\/g, '/')
    .split('/')
    .at(-1)!
    .replace(/\.(?:cmd|exe|ps1)$/i, '')
    .toLowerCase()
}

function skipCommandPrefix(tokens: readonly string[]): number {
  let index = 0
  if (tokens[index] === '&') {
    index += 1
  }
  if (commandName(tokens[index] ?? '') === 'sudo') {
    index += 1
  }
  if (commandName(tokens[index] ?? '') === 'env') {
    index += 1
  }
  while (/^[A-Za-z_][A-Za-z0-9_]*=/.test(tokens[index] ?? '')) {
    index += 1
  }
  return index
}

function directTool(command: string): WorkspaceDiagnosticTool | null {
  const tokens = commandTokens(command)
  let index = skipCommandPrefix(tokens)
  let executable = commandName(tokens[index] ?? '')
  if (['npx', 'bunx'].includes(executable)) {
    index += 1
    executable = commandName(tokens[index] ?? '')
  } else if (['pnpm', 'npm', 'yarn', 'bun'].includes(executable)) {
    const action = (tokens[index + 1] ?? '').toLowerCase()
    if (['exec', 'dlx', 'x'].includes(action)) {
      index += 2
      executable = commandName(tokens[index] ?? '')
    } else {
      const scriptIndex = action === 'run' ? index + 2 : index + 1
      const script = (tokens[scriptIndex] ?? '').toLowerCase()
      if (/^(?:test)(?::|$)/.test(script)) {
        return 'unknown'
      }
      if (/^(?:typecheck|type-check|check:types)(?::|$)/.test(script)) {
        return 'typescript'
      }
      if (/^lint(?::|$)/.test(script)) {
        return 'eslint'
      }
      return null
    }
  }

  const tools: Record<string, WorkspaceDiagnosticTool> = {
    vitest: 'vitest',
    jest: 'jest',
    pytest: 'pytest',
    tsc: 'typescript',
    typescript: 'typescript',
    eslint: 'eslint',
    ruff: 'ruff'
  }
  return tools[executable] ?? null
}

type WorkspaceDiagnosticCommandOperator = 'start' | 'and' | 'or' | 'sequence'

export type WorkspaceDiagnosticCommandStep = {
  tool: WorkspaceDiagnosticTool | null
  precedingOperator: WorkspaceDiagnosticCommandOperator
}

export type WorkspaceDiagnosticCommandAnalysis = WorkspaceDiagnosticCommandStep & {
  tool: WorkspaceDiagnosticTool
  selectedStepIndex: number
  steps: WorkspaceDiagnosticCommandStep[]
}

export function analyzeWorkspaceDiagnosticCommand(
  command: string
): WorkspaceDiagnosticCommandAnalysis | null {
  const clean = cleanDiagnosticsOutput(command)
  const segments: {
    command: string
    precedingOperator: WorkspaceDiagnosticCommandOperator
  }[] = []
  const operatorPattern = /&&|\|\||;|\r?\n/g
  let precedingOperator: WorkspaceDiagnosticCommandOperator = 'start'
  let segmentStart = 0
  for (const match of clean.matchAll(operatorPattern)) {
    segments.push({ command: clean.slice(segmentStart, match.index), precedingOperator })
    precedingOperator = match[0] === '&&' ? 'and' : match[0] === '||' ? 'or' : 'sequence'
    segmentStart = (match.index ?? 0) + match[0].length
  }
  segments.push({ command: clean.slice(segmentStart), precedingOperator })
  const steps = segments.map(
    (segment): WorkspaceDiagnosticCommandStep => ({
      tool: directTool(segment.command),
      precedingOperator: segment.precedingOperator
    })
  )
  for (let index = steps.length - 1; index >= 0; index -= 1) {
    const step = steps[index]!
    const tool = step.tool
    if (tool) {
      return {
        tool,
        precedingOperator: step.precedingOperator,
        selectedStepIndex: index,
        steps
      }
    }
  }
  return null
}

export function detectWorkspaceDiagnosticTool(command: string): WorkspaceDiagnosticTool | null {
  return analyzeWorkspaceDiagnosticCommand(command)?.tool ?? null
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
