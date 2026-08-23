// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { DashboardCard } from '../../../../shared/dashboard-snapshot'
import { MissionControlView } from './MissionControlView'

function card(overrides: Partial<DashboardCard> = {}): DashboardCard {
  return {
    paneKey: 'tab-1:leaf-1',
    ptyId: 'pty-1',
    agentType: 'codex',
    bucket: 'attention',
    dotState: 'blocked',
    task: 'Plan rollout',
    repoId: 'repo-1',
    worktreeId: 'worktree-1',
    tabId: 'tab-1',
    leafId: 'leaf-1',
    repoName: 'MCode',
    worktreeName: 'mission-control',
    hostKind: 'ssh',
    executionHostId: 'ssh:builder',
    hostLabel: 'Builder',
    startedAt: 100,
    finishedAt: null,
    stateChangedAt: 500,
    unseen: true,
    askSummary: JSON.stringify({
      question: 'Which rollout should we use?',
      options: ['Canary', 'All at once']
    }),
    orchestration: {
      taskId: 'task-1',
      dispatchId: 'dispatch-1',
      dispatchStatus: 'dispatched',
      taskTitle: 'Plan rollout',
      displayName: 'Rollout worker',
      orchestrationRunId: 'run-123456'
    },
    ...overrides
  }
}

afterEach(cleanup)

describe('MissionControlView', () => {
  it('shows a decision inbox and mission progress, then opens the exact agent', () => {
    const pending = card()
    const onOpenTerminal = vi.fn()

    render(
      <MissionControlView
        cards={[pending]}
        onOpenTerminal={onOpenTerminal}
        onOpenAgentMap={vi.fn()}
      />
    )

    expect(screen.getByRole('heading', { name: 'Decision Inbox' })).toBeInTheDocument()
    expect(screen.getByText('Which rollout should we use?')).toBeInTheDocument()
    expect(screen.getByText('Canary')).toBeInTheDocument()
    expect(screen.getByText('All at once')).toBeInTheDocument()
    expect(screen.getAllByText(/Rollout worker/)).not.toHaveLength(0)
    expect(screen.getByRole('heading', { name: 'Mission run-1234' })).toBeInTheDocument()
    expect(screen.getByRole('progressbar', { name: 'Mission run-1234 progress' })).toHaveAttribute(
      'aria-valuenow',
      '0'
    )
    expect(screen.getByText('1 mission')).toBeInTheDocument()
    expect(screen.getAllByText('1 decision')).not.toHaveLength(0)
    expect(screen.getByText('1 Needs you')).toBeInTheDocument()
    expect(screen.queryByText('0 Failed')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Review decision: Which rollout should we use?' }))

    expect(onOpenTerminal).toHaveBeenCalledWith(pending)
  })

  it('explains empty and unassigned states without inventing missions', () => {
    const independent = card({
      paneKey: 'plain-agent',
      bucket: 'working',
      dotState: 'working',
      askSummary: undefined,
      orchestration: undefined
    })
    const onOpenTerminal = vi.fn()
    render(
      <MissionControlView
        cards={[independent]}
        onOpenTerminal={onOpenTerminal}
        onOpenAgentMap={vi.fn()}
      />
    )

    expect(screen.getByText('No decisions are waiting.')).toBeInTheDocument()
    expect(screen.getByText('No visible mission runs yet.')).toBeInTheDocument()
    expect(screen.getByText('1 agent is not linked to a mission run.')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Open an agent to dispatch work' }))
    expect(onOpenTerminal).toHaveBeenCalledWith(independent)
  })

  it('routes an empty fleet to the Agent Map where an agent can be started', () => {
    const onOpenAgentMap = vi.fn()

    render(
      <MissionControlView
        cards={[]}
        onOpenTerminal={vi.fn()}
        onOpenAgentMap={onOpenAgentMap}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open Agent Map to start an agent' }))
    expect(onOpenAgentMap).toHaveBeenCalledOnce()
  })
})
