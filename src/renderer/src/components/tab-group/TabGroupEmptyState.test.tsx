// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { launchMock, openSettingsPageMock, openSettingsTargetMock, openUrlMock, detectedIdsBox } =
  vi.hoisted(() => ({
    launchMock: vi.fn(),
    openSettingsPageMock: vi.fn(),
    openSettingsTargetMock: vi.fn(),
    openUrlMock: vi.fn(),
    detectedIdsBox: { value: ['claude', 'codex'] as string[] | null }
  }))

vi.mock('@/i18n/i18n', () => ({ translate: (_key: string, fallback: string) => fallback }))
vi.mock('sonner', () => ({ toast: { error: vi.fn(), message: vi.fn() } }))
vi.mock('@/store', () => ({
  useAppStore: (selector: (state: unknown) => unknown) =>
    selector({
      settings: { defaultTuiAgent: 'claude', disabledTuiAgents: [] },
      openSettingsPage: openSettingsPageMock,
      openSettingsTarget: openSettingsTargetMock
    })
}))
vi.mock('@/hooks/useAgentDetectionTarget', () => ({
  useAgentDetectionTargetForWorktree: () => null
}))
vi.mock('@/hooks/useDetectedAgents', () => ({
  useDetectedAgents: () => ({
    detectedIds: detectedIdsBox.value,
    isLoading: false,
    detectionFailed: false,
    isRefreshing: false,
    refresh: vi.fn(async () => [])
  })
}))
vi.mock('@/components/tab-bar/QuickLaunchButton', () => ({
  buildQuickLaunchAgentMenuEntries: (_defaultAgent: unknown, detected: string[]) => [
    ...detected.map((agent) => ({ agent, action: 'launch' as const })),
    { agent: 'opencode', action: 'install' as const }
  ]
}))
vi.mock('@/lib/agent-catalog', () => ({
  AgentIcon: () => null,
  getAgentCatalog: () => [
    { id: 'claude', label: 'Claude' },
    { id: 'codex', label: 'Codex' },
    { id: 'opencode', label: 'OpenCode', homepageUrl: 'https://opencode.ai' }
  ]
}))
vi.mock('@/lib/launch-agent-in-new-tab', () => ({
  launchAgentInNewTab: launchMock
}))

import TabGroupEmptyState from './TabGroupEmptyState'

// The component marks interactive nodes with domain data-* attributes, not
// data-testid, so queries go through attribute selectors.
function required<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector)
  if (!element) {
    throw new Error(`expected element for selector ${selector}`)
  }
  return element
}

let launchCounter = 0

function renderEmptyState(): { activated: string[] } {
  const activated: string[] = []
  render(
    <TabGroupEmptyState
      worktreeId="wt-1"
      groupId="group-1"
      onNewTerminal={() => {}}
      onNewBrowser={() => {}}
      onNewMarkdown={() => {}}
      onActivateTerminal={(terminalId) => {
        activated.push(terminalId)
      }}
    />
  )
  return { activated }
}

type LaunchAgentArgs = {
  agent: string
  worktreeId: string
  groupId: string
  launchSource: string
}

function launchedAgents(): string[] {
  return launchMock.mock.calls.map((call) => {
    const args: LaunchAgentArgs = call[0]
    return args.agent
  })
}

beforeEach(() => {
  launchCounter = 0
  launchMock.mockReset().mockImplementation(() => {
    launchCounter += 1
    return { tabId: `tab-${launchCounter}` }
  })
  openSettingsPageMock.mockReset()
  openSettingsTargetMock.mockReset()
  openUrlMock.mockReset().mockResolvedValue(undefined)
  Object.defineProperty(window, 'api', {
    configurable: true,
    value: { shell: { openUrl: openUrlMock } }
  })

})

afterEach(cleanup)

describe('TabGroupEmptyState', () => {
  it('renders one launch row per detected agent with a per-agent session count', () => {
    renderEmptyState()

    expect(screen.getByText('Claude')).toBeInTheDocument()
    expect(screen.getByText('Codex')).toBeInTheDocument()
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('1')
    // Undetected featured agents render as install entries, not launch rows.
    expect(required('[data-agent-install-row="opencode"]')).toBeInTheDocument()
    expect(document.querySelector('[data-agent-launch-row="opencode"]')).toBeNull()
  })

  it('launches the chosen number of sessions and focuses the last created tab', () => {
    const { activated } = renderEmptyState()

    fireEvent.click(required('[data-agent-count-increment="claude"]'))
    fireEvent.click(required('[data-agent-count-increment="claude"]'))
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('3')

    fireEvent.click(required('[data-agent-launch-row="claude"]'))

    expect(launchMock).toHaveBeenCalledTimes(3)
    expect(launchMock).toHaveBeenCalledWith(
      expect.objectContaining({ agent: 'claude', worktreeId: 'wt-1', groupId: 'group-1' })
    )
    expect(activated).toEqual(['tab-3'])
    expect(launchedAgents()).toEqual(['claude', 'claude', 'claude'])
  })

  it('clamps the session count to 1..10', () => {
    renderEmptyState()

    const increment = required<HTMLButtonElement>('[data-agent-count-increment="claude"]')
    for (let index = 0; index < 15; index += 1) {
      fireEvent.click(increment)
    }
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('10')
    expect(increment).toBeDisabled()

    const decrement = required<HTMLButtonElement>('[data-agent-count-decrement="claude"]')
    for (let index = 0; index < 15; index += 1) {
      fireEvent.click(decrement)
    }
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('1')
    expect(decrement).toBeDisabled()
  })

  it('opens the install page for undetected featured agents', async () => {
    renderEmptyState()

    fireEvent.click(required('[data-agent-install-row="opencode"]'))

    await vi.waitFor(() => expect(openUrlMock).toHaveBeenCalledWith('https://opencode.ai'))
  })

  it('links to agent settings via the shared settings target', () => {
    renderEmptyState()

    fireEvent.click(required('[data-empty-state-agent-settings]'))

    expect(openSettingsTargetMock).toHaveBeenCalledWith({ pane: 'agents', repoId: null })
    expect(openSettingsPageMock).toHaveBeenCalledTimes(1)
  })

  it('explains when no agents are detected instead of rendering empty rows', () => {
    detectedIdsBox.value = []
    try {
      renderEmptyState()
      expect(screen.getByText('No agents detected')).toBeInTheDocument()
      expect(document.querySelector('[data-agent-launch-row="claude"]')).toBeNull()
    } finally {
      detectedIdsBox.value = ['claude', 'codex']
    }
  })
})
