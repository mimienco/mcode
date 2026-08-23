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
})
