// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { FLOATING_TERMINAL_WORKTREE_ID } from '../../../../shared/constants'

const mocks = vi.hoisted(() => {
  const state = {
    recentQuickCommandIdByGroup: {} as Record<string, string>,
    repos: [] as { id: string }[],
    setRecentQuickCommandForGroup: vi.fn(),
    upsertTerminalQuickCommand: vi.fn(),
    deleteTerminalQuickCommand: vi.fn()
  }
  const useAppStore = Object.assign(
    (selector: (value: typeof state) => unknown) => selector(state),
    { getState: () => state }
  )
  return { state, useAppStore }
})

vi.mock('@/store', () => ({ useAppStore: mocks.useAppStore }))

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: ReactNode }) => <div>{children}</div>
}))

vi.mock('@/components/confirmation-dialog-context', () => ({
  useConfirmationDialog: () => vi.fn()
}))

vi.mock('@/hooks/use-terminal-quick-command-hosts', () => ({
  flattenTerminalQuickCommandHosts: () => [],
  useTerminalQuickCommandHosts: () => ({
    executionHostId: 'local',
    hosts: [{ hostId: 'local', label: 'This computer', commands: [] }],
    refreshRemoteHost: vi.fn(),
    remoteHostLoadFailed: false,
    remoteHostPending: false
  })
}))

vi.mock('@/store/selectors', () => ({
  useProjectHostSetupProjection: () => ({ setups: [] })
}))

vi.mock('../../../../shared/worktree/id', () => ({
  getRepoIdFromWorktreeId: () => 'repo-1'
}))

vi.mock('@/components/terminal-quick-commands/TerminalQuickCommandDialog', () => ({
  createTerminalQuickCommandDraft: (scope = { type: 'global' }) => ({
    id: 'draft',
    label: '',
    action: 'terminal-command',
    command: '',
    appendEnter: true,
    scope
  }),
  TerminalQuickCommandDialog: ({
    open,
    command
  }: {
    open: boolean
    command: { scope: { type: string; repoId?: string } }
  }) =>
    open ? (
      <div
        data-testid="quick-command-dialog"
        data-scope={command.scope.type}
        data-repo-id={command.scope.repoId}
      />
    ) : null
}))

vi.mock('./TabBarQuickCommandsMenu', () => ({
  TabBarQuickCommandsMenu: () => <div data-testid="quick-command-menu" />
}))

vi.mock('@/lib/run-quick-command-in-new-tab', () => ({
  runQuickCommandInNewTab: vi.fn()
}))

import { TabBarQuickCommandsButton } from './TabBarQuickCommandsButton'

afterEach(() => cleanup())

beforeEach(() => {
  mocks.state.repos = []
  vi.clearAllMocks()
})

describe('TabBarQuickCommandsButton empty state', () => {
  it('offers a global command in a no-project chat or terminal', () => {
    render(
      <TabBarQuickCommandsButton
        worktreeId={FLOATING_TERMINAL_WORKTREE_ID}
        groupId="floating-group"
        targetTerminalTabId="terminal-1"
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Add quick command' }))

    expect(screen.getByTestId('quick-command-dialog').getAttribute('data-scope')).toBe('global')
  })

  it('offers a repo command in a project terminal or chat', () => {
    mocks.state.repos = [{ id: 'repo-1' }]
    render(
      <TabBarQuickCommandsButton
        worktreeId="repo-1:worktree"
        groupId="project-group"
        targetTerminalTabId="terminal-1"
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Add quick command' }))

    expect(screen.getByTestId('quick-command-dialog').getAttribute('data-scope')).toBe('repo')
    expect(screen.getByTestId('quick-command-dialog').getAttribute('data-repo-id')).toBe('repo-1')
  })
})
