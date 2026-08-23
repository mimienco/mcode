import { beforeEach, describe, expect, it } from 'vitest'

import {
  beginWorkspaceDiagnosticsCommand,
  captureWorkspaceTerminalOutput,
  completeWorkspaceDiagnosticsCommand,
  resetWorkspaceDiagnosticsForTest,
  useWorkspaceDiagnosticsStore,
  workspaceDiagnosticsIdentity
} from './workspace-diagnostics'

const WORKSPACE_KEY = workspaceDiagnosticsIdentity(undefined, 'wt-1')

function commandRuns() {
  const summary = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
  return Object.values(summary?.runsByTool ?? {}).filter((entry) => entry?.tool === 'command')
}

describe('workspace diagnostics capture', () => {
  beforeEach(() => resetWorkspaceDiagnosticsForTest())

  it('commits the latest diagnostic command and replaces failures after a passing rerun', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      command: 'vitest run',
      cwd: '/repo/packages/cart'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: ' FAIL src/cart.test.ts > cart > totals\r\nAssertionError: wrong total\r\n'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: ' ❯ src/cart.test.ts:42:3\r\nTests  1 failed | 2 passed (3)\r\n'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      cwd: '/repo',
      exitCode: 1
    })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      tools: ['vitest'],
      status: 'failed',
      failed: 1
    })
    expect(
      useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]?.diagnostics[0]
    ).toMatchObject({ basePath: '/repo/packages/cart' })

    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: 'Tests  3 passed (3)\r\n'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      status: 'passed',
      failed: 0,
      passed: 3,
      diagnostics: []
    })
  })

  it('completes only the PTY that emitted the command-finished event', () => {
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      data: 'Tests  4 passed (4)\n'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-lint',
      worktreeId: 'wt-1',
      data: 'eslint\nsrc/a.ts\n  2:1  error  Broken  demo/rule\n'
    })

    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-tests', worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      tools: ['vitest'],
      status: 'passed'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-lint', worktreeId: 'wt-1', exitCode: 1 })
    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      tools: ['vitest', 'eslint'],
      status: 'failed',
      failed: 1
    })
  })

  it('clears a tool failure when a recognized silent rerun succeeds', () => {
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: 'tsc --noEmit\nsrc/a.ts:2:1 - error TS2322: Broken\n'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 2 })
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      command: 'tsc --noEmit'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      status: 'passed',
      failed: 0,
      diagnostics: []
    })
  })

  it('does not clear a failure when unrelated output merely mentions a tool', () => {
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: 'src/a.ts\n  2:1  error  Broken  demo/rule\n'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 1 })
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      command: 'cat package.json'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: '{ "eslint": "latest" }\n'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      status: 'failed',
      failed: 1
    })
  })

  it('keeps same-id workspaces on separate hosts isolated', () => {
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-a',
      worktreeId: 'wt-1',
      hostId: 'runtime:host-a',
      data: 'Tests  1 failed (1)\n'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-a',
      worktreeId: 'wt-1',
      hostId: 'runtime:host-a',
      exitCode: 1
    })

    expect(
      useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[
        workspaceDiagnosticsIdentity('runtime:host-a', 'wt-1')
      ]
    ).toBeDefined()
    expect(
      useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[
        workspaceDiagnosticsIdentity('runtime:host-b', 'wt-1')
      ]
    ).toBeUndefined()
  })

  it('surfaces a package test command that exits before the runner prints a summary', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      command: 'pnpm test'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      data: 'Error: failed to load test configuration\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      tools: ['unknown'],
      status: 'failed',
      failed: 1,
      outputPtyId: 'pty-tests'
    })
  })

  it('prefers a mounted pane live cwd after a compound cd command', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      command: 'cd packages/api && pnpm test',
      cwd: '/repo'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      data:
        'FAIL src/api.test.ts > api > rejects invalid input\nError: rejected\n' +
        ' ❯ src/api.test.ts:8:2\nTests  1 failed (1)\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      cwd: '/repo/packages/api',
      cwdIsLive: true,
      exitCode: 1
    })

    expect(
      useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]?.diagnostics[0]
    ).toMatchObject({ basePath: '/repo/packages/api' })
  })

  it('adds a locationless result for an unresolved failing step in a combined command', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: 'vitest run && eslint src'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Tests  20 passed (20)\nESLint: failed to load configuration\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 2
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(run?.runsByTool.vitest).toMatchObject({ status: 'passed', passed: 20 })
    expect(run?.runsByTool.eslint).toMatchObject({ status: 'failed', failed: 1, diagnostics: [] })
  })

  it('does not report a later && step when an earlier parsed step failed', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: 'vitest run && eslint src'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Tests  1 failed (1)\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(run?.runsByTool.vitest).toMatchObject({ status: 'failed', failed: 1 })
    expect(run?.runsByTool.eslint).toBeUndefined()
  })

  it('uses a generic failure when an earlier && step fails without parser evidence', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: 'vitest run && eslint src'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Vitest failed to load config\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(commandRuns()).toEqual([expect.objectContaining({ status: 'failed', diagnostics: [] })])
    expect(run?.runsByTool.eslint).toBeUndefined()
  })

  it('does not blame a passing check for a later non-diagnostic failure', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: 'vitest run && node scripts/check-env.js'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Tests  20 passed (20)\nEnvironment check failed\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(run?.runsByTool.vitest).toMatchObject({ status: 'passed', passed: 20 })
    expect(commandRuns()).toEqual([expect.objectContaining({ status: 'failed', diagnostics: [] })])
  })

  it('clears a generic chain failure after the diagnostic command recovers', () => {
    const command = 'vitest run && node scripts/check-env.js'
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Tests  2 passed (2)\nEnvironment check failed\n'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Tests  2 passed (2)\nEnvironment check passed\n'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 0
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(run?.runsByTool.vitest).toMatchObject({ status: 'passed', passed: 2 })
    expect(commandRuns()).toEqual([])
  })

  it('keeps a generic chain failure after an unrelated check passes', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: 'vitest run && node scripts/check-env.js'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      data: 'Tests  2 passed (2)\nEnvironment check failed\n'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: 'eslint src'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 0
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(run?.runsByTool.eslint).toMatchObject({ status: 'passed' })
    expect(commandRuns()).toEqual([expect.objectContaining({ status: 'failed' })])
  })

  it('retains each unresolved pipeline until that same pipeline recovers', () => {
    const firstCommand = 'vitest run && node scripts/check-env.js'
    const secondCommand = 'eslint src && node scripts/check-generated.js'
    for (const command of [firstCommand, secondCommand]) {
      beginWorkspaceDiagnosticsCommand({ ptyId: 'pty-checks', worktreeId: 'wt-1', command })
      completeWorkspaceDiagnosticsCommand({
        ptyId: 'pty-checks',
        worktreeId: 'wt-1',
        exitCode: 1
      })
    }

    expect(commandRuns()).toHaveLength(2)

    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      command: secondCommand
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-checks',
      worktreeId: 'wt-1',
      exitCode: 0
    })

    expect(commandRuns()).toEqual([
      expect.objectContaining({ commandIdentity: firstCommand, status: 'failed' })
    ])
  })

  it('retires a generic package-test failure when a concrete runner recovers', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      command: 'pnpm test'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      command: 'pnpm test'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      data: 'Tests  4 passed (4)\n'
    })
    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-tests',
      worktreeId: 'wt-1',
      exitCode: 0
    })

    const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
    expect(run).toMatchObject({ status: 'passed', tools: ['vitest'], passed: 4 })
    expect(run?.runsByTool.unknown).toBeUndefined()
  })

  it('keeps a represented check failed when its own process exits nonzero', () => {
    beginWorkspaceDiagnosticsCommand({
      ptyId: 'pty-lint',
      worktreeId: 'wt-1',
      command: 'eslint --max-warnings=0 src'
    })
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-lint',
      worktreeId: 'wt-1',
      data: 'src/a.ts\n  2:1  warning  Avoid this  demo/rule\n'
    })

    completeWorkspaceDiagnosticsCommand({
      ptyId: 'pty-lint',
      worktreeId: 'wt-1',
      exitCode: 1
    })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]).toMatchObject({
      status: 'failed',
      failed: 1,
      outputPtyId: 'pty-lint'
    })
  })

  it.each(['vitest run; eslint src', 'vitest run || eslint src'])(
    'keeps an unparsed later failure after an earlier parsed failure for %s',
    (command) => {
      beginWorkspaceDiagnosticsCommand({ ptyId: 'pty-checks', worktreeId: 'wt-1', command })
      captureWorkspaceTerminalOutput({
        ptyId: 'pty-checks',
        worktreeId: 'wt-1',
        data: 'Tests  1 failed (1)\nESLint: failed to load configuration\n'
      })

      completeWorkspaceDiagnosticsCommand({
        ptyId: 'pty-checks',
        worktreeId: 'wt-1',
        exitCode: 2
      })

      const run = useWorkspaceDiagnosticsStore.getState().runsByWorktreeId[WORKSPACE_KEY]
      expect(run?.runsByTool.vitest).toMatchObject({ status: 'failed' })
      expect(run?.runsByTool.eslint).toMatchObject({ status: 'failed', diagnostics: [] })
    }
  )
})
