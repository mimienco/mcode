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
    completeWorkspaceDiagnosticsCommand({ worktreeId: 'wt-1', exitCode: 1 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
      tool: 'vitest',
      status: 'failed',
      failed: 1
    })

    captureWorkspaceTerminalOutput({
      ptyId: 'pty-1',
      worktreeId: 'wt-1',
      data: 'Tests  3 passed (3)\r\n'
    })
    completeWorkspaceDiagnosticsCommand({ worktreeId: 'wt-1', exitCode: 0 })

    expect(useWorkspaceDiagnosticsStore.getState().runsByWorktreeId['wt-1']).toMatchObject({
      status: 'passed',
      failed: 0,
      passed: 3,
      diagnostics: []
    })
  })
})
