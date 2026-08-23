import { create } from 'zustand'
import type { ExecutionHostId } from '../../../shared/execution-host'
import { composeWorktreeHostIdentity } from '../../../shared/worktree/host-qualified-identity'

import {
  detectWorkspaceDiagnosticTool,
  parseWorkspaceDiagnosticsRunsOutput,
  type WorkspaceDiagnostic,
  type WorkspaceDiagnosticsRun,
  type WorkspaceDiagnosticTool
} from '@/lib/workspace-diagnostics-parser'

const MAX_CAPTURE_CHARS = 256 * 1024
const MAX_CAPTURED_TERMINALS = 64

export type WorkspaceDiagnosticsEntry = WorkspaceDiagnosticsRun & {
  ptyId: string
  updatedAt: number
}

export type WorkspaceDiagnosticsSummary = Omit<WorkspaceDiagnosticsRun, 'tool'> & {
  tools: WorkspaceDiagnosticTool[]
  updatedAt: number
  outputPtyId: string | null
  runsByTool: Partial<Record<WorkspaceDiagnosticTool, WorkspaceDiagnosticsEntry>>
}

type WorkspaceDiagnosticsState = {
  runsByWorktreeId: Record<string, WorkspaceDiagnosticsSummary>
  clearWorktree: (worktreeId: string, hostId?: ExecutionHostId) => void
}

type TerminalOutputCapture = {
  chunks: string[]
  charCount: number
  headIndex: number
  revision: number
  workspaceIdentity: string
}

type TerminalCommandMetadata = {
  command: string
  cwd?: string
  workspaceIdentity: string
}

const capturesByPtyId = new Map<string, TerminalOutputCapture>()
const commandMetadataByPtyId = new Map<string, TerminalCommandMetadata>()
let captureRevision = 0

export const useWorkspaceDiagnosticsStore = create<WorkspaceDiagnosticsState>((set) => ({
  runsByWorktreeId: {},
  clearWorktree: (worktreeId, hostId) =>
    set((state) => {
      const workspaceIdentity = workspaceDiagnosticsIdentity(hostId, worktreeId)
      if (!state.runsByWorktreeId[workspaceIdentity]) {
        return state
      }
      const runsByWorktreeId = { ...state.runsByWorktreeId }
      delete runsByWorktreeId[workspaceIdentity]
      return { runsByWorktreeId }
    })
}))

function trimCapture(capture: TerminalOutputCapture): void {
  while (capture.charCount > MAX_CAPTURE_CHARS && capture.headIndex < capture.chunks.length) {
    const excess = capture.charCount - MAX_CAPTURE_CHARS
    const first = capture.chunks[capture.headIndex]!
    if (first.length <= excess) {
      capture.headIndex += 1
      capture.charCount -= first.length
    } else {
      capture.chunks[capture.headIndex] = first.slice(excess)
      capture.charCount -= excess
    }
  }
  if (capture.headIndex > 1024 && capture.headIndex * 2 > capture.chunks.length) {
    capture.chunks = capture.chunks.slice(capture.headIndex)
    capture.headIndex = 0
  }
}

export function workspaceDiagnosticsIdentity(
  hostId: ExecutionHostId | undefined,
  worktreeId: string
): string {
  return composeWorktreeHostIdentity(hostId, worktreeId)
}

export function captureWorkspaceTerminalOutput(args: {
  ptyId: string
  worktreeId: string
  hostId?: ExecutionHostId
  data: string
}): void {
  if (!args.data) {
    return
  }
  const workspaceIdentity = workspaceDiagnosticsIdentity(args.hostId, args.worktreeId)
  const capture = capturesByPtyId.get(args.ptyId) ?? {
    chunks: [],
    charCount: 0,
    headIndex: 0,
    revision: 0,
    workspaceIdentity
  }
  capture.chunks.push(args.data)
  capture.charCount += args.data.length
  captureRevision += 1
  capture.revision = captureRevision
  trimCapture(capture)
  capturesByPtyId.set(args.ptyId, capture)
  if (capturesByPtyId.size > MAX_CAPTURED_TERMINALS) {
    const oldestPtyId = [...capturesByPtyId.entries()].reduce((oldest, candidate) =>
      candidate[1].revision < oldest[1].revision ? candidate : oldest
    )[0]
    capturesByPtyId.delete(oldestPtyId)
    commandMetadataByPtyId.delete(oldestPtyId)
  }
}

export function beginWorkspaceDiagnosticsCommand(args: {
  ptyId: string
  worktreeId: string
  hostId?: ExecutionHostId
  command: string
  cwd?: string
}): void {
  const workspaceIdentity = workspaceDiagnosticsIdentity(args.hostId, args.worktreeId)
  captureRevision += 1
  capturesByPtyId.set(args.ptyId, {
    chunks: [],
    charCount: 0,
    headIndex: 0,
    revision: captureRevision,
    workspaceIdentity
  })
  commandMetadataByPtyId.set(args.ptyId, {
    command: args.command,
    workspaceIdentity,
    ...(args.cwd ? { cwd: args.cwd } : {})
  })
}

function aggregateRuns(
  runsByTool: WorkspaceDiagnosticsSummary['runsByTool']
): WorkspaceDiagnosticsSummary {
  const entries = Object.values(runsByTool).filter(
    (entry): entry is WorkspaceDiagnosticsEntry => entry !== undefined
  )
  const newestFailedEntry = [...entries]
    .sort((left, right) => right.updatedAt - left.updatedAt)
    .find((entry) => entry.status === 'failed')
  return {
    runsByTool,
    tools: entries.map((entry) => entry.tool),
    status: entries.some((entry) => entry.status === 'failed') ? 'failed' : 'passed',
    failed: entries.reduce((sum, entry) => sum + entry.failed, 0),
    passed: entries.reduce((sum, entry) => sum + entry.passed, 0),
    total: entries.reduce((sum, entry) => sum + entry.total, 0),
    diagnostics: entries.flatMap((entry) => entry.diagnostics),
    updatedAt: Math.max(...entries.map((entry) => entry.updatedAt)),
    outputPtyId: newestFailedEntry?.ptyId ?? null
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
  hostId?: ExecutionHostId
  cwd?: string
  exitCode: number | null
  now?: number
}): WorkspaceDiagnosticsEntry[] {
  const capture = capturesByPtyId.get(args.ptyId)
  const metadata = commandMetadataByPtyId.get(args.ptyId)
  const workspaceIdentity =
    metadata?.workspaceIdentity ?? workspaceDiagnosticsIdentity(args.hostId, args.worktreeId)
  if (
    (!capture && !metadata) ||
    (capture && capture.workspaceIdentity !== workspaceIdentity) ||
    (metadata && metadata.workspaceIdentity !== workspaceIdentity)
  ) {
    return []
  }
  capturesByPtyId.delete(args.ptyId)
  commandMetadataByPtyId.delete(args.ptyId)
  const output = capture?.chunks.slice(capture.headIndex).join('') ?? ''
  const toolHint = metadata ? detectWorkspaceDiagnosticTool(metadata.command) : null
  const parsedRuns = parseWorkspaceDiagnosticsRunsOutput(output, args.exitCode)
  const runs =
    parsedRuns.length > 0 ? parsedRuns : toolHint ? [synthesizeRun(toolHint, args.exitCode)] : []
  if (runs.length === 0) {
    return []
  }
  const updatedAt = args.now ?? Date.now()
  const basePath = metadata?.cwd ?? args.cwd
  const entries = runs.map(
    (run): WorkspaceDiagnosticsEntry => ({
      ...run,
      ptyId: args.ptyId,
      diagnostics: attachBasePath(run.diagnostics, basePath),
      updatedAt
    })
  )
  useWorkspaceDiagnosticsStore.setState((state) => {
    const prior = state.runsByWorktreeId[workspaceIdentity]?.runsByTool ?? {}
    const runsByTool = { ...prior }
    for (const entry of entries) {
      runsByTool[entry.tool] = entry
    }
    return {
      runsByWorktreeId: {
        ...state.runsByWorktreeId,
        [workspaceIdentity]: aggregateRuns(runsByTool)
      }
    }
  })
  return entries
}

export function resetWorkspaceDiagnosticsForTest(): void {
  capturesByPtyId.clear()
  commandMetadataByPtyId.clear()
  captureRevision = 0
  useWorkspaceDiagnosticsStore.setState({ runsByWorktreeId: {} })
}

export function pruneWorkspaceDiagnostics(validWorkspaceIdentities: ReadonlySet<string>): void {
  useWorkspaceDiagnosticsStore.setState((state) => {
    const runsByWorktreeId = Object.fromEntries(
      Object.entries(state.runsByWorktreeId).filter(([identity]) =>
        validWorkspaceIdentities.has(identity)
      )
    )
    return Object.keys(runsByWorktreeId).length === Object.keys(state.runsByWorktreeId).length
      ? state
      : { runsByWorktreeId }
  })
  for (const [ptyId, capture] of capturesByPtyId) {
    if (!validWorkspaceIdentities.has(capture.workspaceIdentity)) {
      capturesByPtyId.delete(ptyId)
      commandMetadataByPtyId.delete(ptyId)
    }
  }
}
