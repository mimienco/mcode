import { create } from 'zustand'

import {
  parseWorkspaceDiagnosticsOutput,
  type WorkspaceDiagnosticsRun
} from '@/lib/workspace-diagnostics-parser'

const MAX_CAPTURE_CHARS = 256 * 1024
const MAX_CAPTURED_TERMINALS = 64

export type WorkspaceDiagnosticsEntry = WorkspaceDiagnosticsRun & {
  updatedAt: number
}

type WorkspaceDiagnosticsState = {
  runsByWorktreeId: Record<string, WorkspaceDiagnosticsEntry>
  clearWorktree: (worktreeId: string) => void
}

type TerminalOutputCapture = {
  output: string
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

export function captureWorkspaceTerminalOutput(args: {
  ptyId: string
  worktreeId: string
  data: string
  now?: number
}): void {
  if (!args.data) {
    return
  }
  const previous = capturesByPtyId.get(args.ptyId)
  const output = `${previous?.output ?? ''}${args.data}`.slice(-MAX_CAPTURE_CHARS)
  captureRevision += 1
  capturesByPtyId.set(args.ptyId, {
    output,
    revision: captureRevision,
    worktreeId: args.worktreeId
  })
  if (capturesByPtyId.size > MAX_CAPTURED_TERMINALS) {
    const oldestPtyId = [...capturesByPtyId.entries()].reduce((oldest, candidate) =>
      candidate[1].revision < oldest[1].revision ? candidate : oldest
    )[0]
    capturesByPtyId.delete(oldestPtyId)
  }
}

export function completeWorkspaceDiagnosticsCommand(args: {
  worktreeId: string
  exitCode: number | null
  now?: number
}): WorkspaceDiagnosticsEntry | null {
  let latest: { ptyId: string; capture: TerminalOutputCapture } | null = null
  for (const [ptyId, capture] of capturesByPtyId) {
    if (
      capture.worktreeId === args.worktreeId &&
      (!latest || capture.revision > latest.capture.revision)
    ) {
      latest = { ptyId, capture }
    }
  }
  if (!latest) {
    return null
  }
  capturesByPtyId.delete(latest.ptyId)
  const run = parseWorkspaceDiagnosticsOutput(latest.capture.output, args.exitCode)
  if (!run) {
    return null
  }
  const entry: WorkspaceDiagnosticsEntry = { ...run, updatedAt: args.now ?? Date.now() }
  useWorkspaceDiagnosticsStore.setState((state) => ({
    runsByWorktreeId: { ...state.runsByWorktreeId, [args.worktreeId]: entry }
  }))
  return entry
}

export function resetWorkspaceDiagnosticsForTest(): void {
  capturesByPtyId.clear()
  captureRevision = 0
  useWorkspaceDiagnosticsStore.setState({ runsByWorktreeId: {} })
}
