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

export function detectWorkspaceDiagnosticTool(command: string): WorkspaceDiagnosticTool | null {
  const segments = cleanDiagnosticsOutput(command).split(/\s*(?:&&|\|\||;|\r?\n)\s*/)
  for (let index = segments.length - 1; index >= 0; index -= 1) {
    const tool = directTool(segments[index] ?? '')
    if (tool) {
      return tool
    }
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
