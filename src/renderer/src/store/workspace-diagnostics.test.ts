import { beforeEach, describe, expect, it } from 'vitest'

import {
  captureWorkspaceTerminalOutput,
  completeWorkspaceDiagnosticsCommand,
  resetWorkspaceDiagnosticsForTest,
  useWorkspaceDiagnosticsStore
} from './workspace-diagnostics'

describe('workspace diagnostics capture', () => {
  beforeEach(() => resetWorkspaceDiagnosticsForTest())

  it('commits the latest diagnostic command and replaces failures after a passing rerun', () => {
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
      cwd: '/repo/packages/cart',
      exitCode: 1
    })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
      tools: ['vitest'],
      status: 'failed',
      failed: 1
    })
    expect(
      useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']?.diagnostics[0]
    ).toMatchObject({ basePath: '/repo/packages/cart' })

    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: 'Tests  3 passed (3)\r\n'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
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

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
      tools: ['vitest'],
      status: 'passed'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-lint', worktreeId: 'wt-1', exitCode: 1 })
    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
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
    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: 'tsc --noEmit\n'
    })
    completeWorkspaceDiagnosticsCommand({ ptyId: 'pty-1', worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
      status: 'passed',
      failed: 0,
      diagnostics: []
    })
  })
})
