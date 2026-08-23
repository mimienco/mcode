import {
  countDiagnosticsSummaryValue,
  createWorkspaceDiagnostic,
  type WorkspaceDiagnostic,
  type WorkspaceDiagnosticsRun
} from './workspace-diagnostics-parser-core'

const VITEST_FAILURE = /^\s*FAIL\s+(.+?)\s+>\s+(.+)$/
const VITEST_FRAME = /^\s*[❯>]\s+(.+?):(\d+):(\d+)\s*$/
const VITEST_TESTS = /^\s*Tests\s+(.+)$/
const JEST_FAILURE_TITLE = /^\s*●\s+(.+)$/
const JEST_FRAME_WITH_CONTEXT = /^\s*at\s+.*?\((.+?):(\d+):(\d+)\)\s*$/
const JEST_FRAME_DIRECT = /^\s*at\s+(.+?):(\d+):(\d+)\s*$/
const JEST_TESTS = /^\s*Tests:\s+(.+)$/i
const PYTEST_FAILURE = /^FAILED\s+(.+?)::(.+?)\s+-\s+(.+)$/
const PYTEST_COLLECTION_ERROR = /^ERROR collecting\s+(.+?)(?:\s+-\s+(.+))?$/
const PYTEST_TEST_ERROR = /^ERROR\s+(.+?)::(.+?)(?:\s+-\s+(.+))?$/
const PYTEST_LOCATION = /^(.+?):(\d+):\s*(.+)$/
const PYTEST_SUMMARY = /={2,}\s*(.+?)\s+in\s+[\d.]+s?\s*=*\s*$/i

export function parsePytestDiagnostics(
  lines: readonly string[],
  _exitCode: number | null
): WorkspaceDiagnosticsRun | null {
  const summaryLine = lines.find(
    (line) => PYTEST_SUMMARY.test(line) && /\b(?:failed|passed|errors?)\b/i.test(line)
  )
  if (!summaryLine) {
    return null
  }
  const summary = summaryLine.match(PYTEST_SUMMARY)![1]
  const locations = lines.flatMap((line) => {
    const match = line.match(PYTEST_LOCATION)
    return match ? [{ filePath: match[1].trim(), line: Number(match[2]) }] : []
  })
  const consumedLocations = new Set<number>()
  const diagnostics: WorkspaceDiagnostic[] = []
  const addDiagnostic = (filePath: string, title: string, message: string): void => {
    const locationIndex = locations.findIndex(
      (candidate, index) => !consumedLocations.has(index) && candidate.filePath === filePath
    )
    if (locationIndex !== -1) {
      consumedLocations.add(locationIndex)
    }
    const location = locationIndex !== -1 ? locations[locationIndex] : undefined
    diagnostics.push(
      createWorkspaceDiagnostic({
        kind: 'test',
        severity: 'error',
        filePath,
        line: location?.line ?? 1,
        column: null,
        title,
        message,
        code: null
      })
    )
  }
  for (const line of lines) {
    const failure = line.match(PYTEST_FAILURE)
    if (failure) {
      addDiagnostic(failure[1].trim(), failure[2].trim(), failure[3].trim())
      continue
    }
    const collectionError = line.match(PYTEST_COLLECTION_ERROR)
    if (collectionError) {
      const filePath = collectionError[1].trim()
      addDiagnostic(filePath, 'Collection error', collectionError[2]?.trim() || 'Collection failed')
      continue
    }
    const testError = line.match(PYTEST_TEST_ERROR)
    if (testError) {
      addDiagnostic(
        testError[1].trim(),
        testError[2].trim(),
        testError[3]?.trim() || 'Test setup or teardown failed'
      )
    }
  }
  const failed =
    countDiagnosticsSummaryValue(summary, 'failed') +
    countDiagnosticsSummaryValue(summary, 'error(?:s)?')
  const passed = countDiagnosticsSummaryValue(summary, 'passed')
  return {
    tool: 'pytest',
    status: failed > 0 ? 'failed' : 'passed',
    failed,
    passed,
    total: failed + passed,
    diagnostics
  }
}

type JestFrame = { filePath: string; line: number; column: number }

function selectJestFrame(frames: readonly JestFrame[]): JestFrame | undefined {
  return (
    frames.find((frame) => /(?:^|[\\/])[^\\/]+\.(?:test|spec)\.[^\\/]+$/i.test(frame.filePath)) ??
    frames.find((frame) => !/[\\/]node_modules[\\/]/.test(frame.filePath)) ??
    frames[0]
  )
}

export function parseJestDiagnostics(
  lines: readonly string[],
  _exitCode: number | null
): WorkspaceDiagnosticsRun | null {
  const summaryLine = lines.find((line) => JEST_TESTS.test(line))
  if (!summaryLine) {
    return null
  }
  const summary = summaryLine.match(JEST_TESTS)![1]
  const diagnostics: WorkspaceDiagnostic[] = []
  let block: { title: string; message: string; frames: JestFrame[] } | null = null
  const flushBlock = (): void => {
    if (!block) {
      return
    }
    const frame = selectJestFrame(block.frames)
    if (frame) {
      diagnostics.push(
        createWorkspaceDiagnostic({
          kind: 'test',
          severity: 'error',
          filePath: frame.filePath,
          line: frame.line,
          column: frame.column,
          title: block.title,
          message: block.message || 'Test failed',
          code: null
        })
      )
    }
    block = null
  }
  for (const line of lines) {
    const titleMatch = line.match(JEST_FAILURE_TITLE)
    if (titleMatch) {
      flushBlock()
      block = { title: titleMatch[1].trim(), message: '', frames: [] }
      continue
    }
    if (!block) {
      continue
    }
    const frame = line.match(JEST_FRAME_WITH_CONTEXT) ?? line.match(JEST_FRAME_DIRECT)
    if (frame) {
      block.frames.push({
        filePath: frame[1].trim(),
        line: Number(frame[2]),
        column: Number(frame[3])
      })
      continue
    }
    if (!block.message && /^\s{4}\S/.test(line)) {
      block.message = line.trim()
    }
  }
  flushBlock()
  const failed = countDiagnosticsSummaryValue(summary, 'failed')
  const passed = countDiagnosticsSummaryValue(summary, 'passed')
  const total = countDiagnosticsSummaryValue(summary, 'total') || failed + passed
  return {
    tool: 'jest',
    status: failed > 0 ? 'failed' : 'passed',
    failed,
    passed,
    total,
    diagnostics
  }
}

export function parseVitestDiagnostics(
  lines: readonly string[],
  exitCode: number | null
): WorkspaceDiagnosticsRun | null {
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
      diagnostics.push(
        createWorkspaceDiagnostic({
          kind: 'test',
          severity: 'error',
          filePath: frame[1].trim(),
          line: Number(frame[2]),
          column: Number(frame[3]),
          title: currentFailure.title,
          message: currentFailure.message || 'Test failed',
          code: null
        })
      )
      currentFailure = null
      continue
    }
    const tests = line.match(VITEST_TESTS)
    if (tests) {
      summary = {
        failed: countDiagnosticsSummaryValue(tests[1], 'failed'),
        passed: countDiagnosticsSummaryValue(tests[1], 'passed')
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
    status: failed > 0 || (!summary && exitCode !== null && exitCode !== 0) ? 'failed' : 'passed',
    failed,
    passed,
    total: failed + passed,
    diagnostics
  }
}
