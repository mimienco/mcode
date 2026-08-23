import { create } from 'zustand'

import {
  detectWorkspaceDiagnosticTool,
  parseWorkspaceDiagnosticsOutput,
  type WorkspaceDiagnostic,
  type WorkspaceDiagnosticsRun,
  type WorkspaceDiagnosticTool
} from '@/lib/workspace-diagnostics-parser'

const MAX_CAPTURE_CHARS = 256 * 1024
const MAX_CAPTURED_TERMINALS = 64

export type WorkspaceDiagnosticsEntry = WorkspaceDiagnosticsRun & {
  updatedAt: number
}

export type WorkspaceDiagnosticsSummary = Omit<WorkspaceDiagnosticsRun, 'tool'> & {
  tools: WorkspaceDiagnosticTool[]
  updatedAt: number
  runsByTool: Partial<Record<WorkspaceDiagnosticTool, WorkspaceDiagnosticsEntry>>
}

type WorkspaceDiagnosticsState = {
  runsByWorktreeId: Record<string, WorkspaceDiagnosticsSummary>
  clearWorktree: (worktreeId: string) => void
}

type TerminalOutputCapture = {
  chunks: string[]
  charCount: number
  revision: number
  worktreeId: string
}

const capturesByPtyId = new Map<string, TerminalOutputCapture>()
let captureRevision = 0

export const useWorkspaceDiagnosticsStore = create<WorkspaceDiagnosticsState>((set) => ({
  runsByWorktreeId: {},
  clearWorktree: (worktreeId) =>
    set((state) => {
      if (!state.runsByWorktreeId[worktreeId]) {
        return state
      }
      const runsByWorktreeId = { ...state.runsByWorktreeId }
      delete runsByWorktreeId[worktreeId]
      return { runsByWorktreeId }
    })
}))

function trimCapture(capture: TerminalOutputCapture): void {
  while (capture.charCount > MAX_CAPTURE_CHARS && capture.chunks.length > 0) {
    const excess = capture.charCount - MAX_CAPTURE_CHARS
    const first = capture.chunks[0]!
    if (first.length <= excess) {
      capture.chunks.shift()
      capture.charCount -= first.length
    } else {
      capture.chunks[0] = first.slice(excess)
      capture.charCount -= excess
    }
  }
}

export function captureWorkspaceTerminalOutput(args: {
  ptyId: string
  worktreeId: string
  data: string
}): void {
  if (!args.data) {
    return
  }
  const capture = capturesByPtyId.get(args.ptyId) ?? {
    chunks: [],
    charCount: 0,
    revision: 0,
    worktreeId: args.worktreeId
  }
  capture.chunks.push(args.data)
  capture.charCount += args.data.length
  capture.worktreeId = args.worktreeId
  captureRevision += 1
  capture.revision = captureRevision
  trimCapture(capture)
  capturesByPtyId.set(args.ptyId, capture)
  if (capturesByPtyId.size > MAX_CAPTURED_TERMINALS) {
    const oldestPtyId = [...capturesByPtyId.entries()].reduce((oldest, candidate) =>
      candidate[1].revision < oldest[1].revision ? candidate : oldest
    )[0]
    capturesByPtyId.delete(oldestPtyId)
  }
}

function aggregateRuns(
  runsByTool: WorkspaceDiagnosticsSummary['runsByTool']
): WorkspaceDiagnosticsSummary {
  const entries = Object.values(runsByTool).filter(
    (entry): entry is WorkspaceDiagnosticsEntry => entry !== undefined
  )
  return {
    runsByTool,
    tools: entries.map((entry) => entry.tool),
    status: entries.some((entry) => entry.status === 'failed') ? 'failed' : 'passed',
    failed: entries.reduce((sum, entry) => sum + entry.failed, 0),
    passed: entries.reduce((sum, entry) => sum + entry.passed, 0),
    total: entries.reduce((sum, entry) => sum + entry.total, 0),
    diagnostics: entries.flatMap((entry) => entry.diagnostics),
    updatedAt: Math.max(...entries.map((entry) => entry.updatedAt))
  }
}

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

function attachBasePath(
  diagnostics: readonly WorkspaceDiagnostic[],
  basePath: string | undefined
): WorkspaceDiagnostic[] {
  if (!basePath) {
    return [...diagnostics]
  }
  return diagnostics.map((diagnostic) => ({ ...diagnostic, basePath }))
}

export function completeWorkspaceDiagnosticsCommand(args: {
  ptyId: string
  worktreeId: string
  cwd?: string
  exitCode: number | null
  now?: number
}): WorkspaceDiagnosticsEntry | null {
  const capture = capturesByPtyId.get(args.ptyId)
  if (!capture || capture.worktreeId !== args.worktreeId) {
    return null
  }
  capturesByPtyId.delete(args.ptyId)
  const output = capture.chunks.join('')
  const toolHint = detectWorkspaceDiagnosticTool(output)
  const parsed = parseWorkspaceDiagnosticsOutput(output, args.exitCode)
  const run = parsed ?? (toolHint ? synthesizeRun(toolHint, args.exitCode) : null)
  if (!run) {
    return null
  }
  const entry: WorkspaceDiagnosticsEntry = {
    ...run,
    diagnostics: attachBasePath(run.diagnostics, args.cwd),
    updatedAt: args.now ?? Date.now()
  }
  useWorkspaceDiagnosticsStore.setState((state) => {
    const prior = state.runsByWorktreeId[args.worktreeId]?.runsByTool ?? {}
    const runsByTool = { ...prior, [entry.tool]: entry }
    return {
      runsByWorktreeId: {
        ...state.runsByWorktreeId,
        [args.worktreeId]: aggregateRuns(runsByTool)
      }
    }
  })
  return entry
}

export function resetWorkspaceDiagnosticsForTest(): void {
  capturesByPtyId.clear()
  captureRevision = 0
  useWorkspaceDiagnosticsStore.setState({ runsByWorktreeId: {} })
}
