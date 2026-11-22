// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const {
  clearPendingWorkspaceLaunchGroupsMock,
  createEmptyGridGroupsMock,
  launchMock,
  openSettingsPageMock,
  openSettingsTargetMock,
  openUrlMock,
  useTemplateMock,
  detectedIdsBox
} = vi.hoisted(() => ({
  clearPendingWorkspaceLaunchGroupsMock: vi.fn(),
  createEmptyGridGroupsMock: vi.fn(),
  launchMock: vi.fn(),
  openSettingsPageMock: vi.fn(),
  openSettingsTargetMock: vi.fn(),
  openUrlMock: vi.fn(),
  useTemplateMock: vi.fn(),
  detectedIdsBox: { value: ['claude', 'codex'] as string[] | null }
}))

vi.mock('@/i18n/i18n', () => ({ translate: (_key: string, fallback: string) => fallback }))
vi.mock('sonner', () => ({ toast: { error: vi.fn(), message: vi.fn() } }))
vi.mock('@/store', () => ({
  useAppStore: (selector: (state: unknown) => unknown) =>
    selector({
      settings: { defaultTuiAgent: 'claude', disabledTuiAgents: [] },
      clearPendingWorkspaceLaunchGroups: clearPendingWorkspaceLaunchGroupsMock,
      createEmptyGridGroups: createEmptyGridGroupsMock,
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
      onUseTemplate={useTemplateMock}
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

function launchedGroupIds(): string[] {
  return launchMock.mock.calls.map((call) => {
    const args: LaunchAgentArgs = call[0]
    return args.groupId
  })
}

let queuedAnimationFrames: FrameRequestCallback[] = []

async function flushNextAnimationFrame(): Promise<void> {
  const callback = queuedAnimationFrames.shift()
  if (!callback) {
    throw new Error('expected a queued animation frame')
  }
  callback(0)
  await Promise.resolve()
}

beforeEach(() => {
  launchCounter = 0
  queuedAnimationFrames = []
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    queuedAnimationFrames.push(callback)
    return queuedAnimationFrames.length
  })
  clearPendingWorkspaceLaunchGroupsMock.mockReset()
  createEmptyGridGroupsMock.mockReset().mockImplementation(
    (_worktreeId: string, sourceGroupId: string, agents: string[]) => [
      sourceGroupId,
      ...Array.from({ length: agents.length - 1 }, (_, index) => `group-${index + 2}`)
    ]
  )
  launchMock.mockReset().mockImplementation(() => {
    launchCounter += 1
    return { tabId: `tab-${launchCounter}` }
  })
  openSettingsPageMock.mockReset()
  openSettingsTargetMock.mockReset()
  openUrlMock.mockReset().mockResolvedValue(undefined)
  useTemplateMock.mockReset().mockResolvedValue(undefined)
  Object.defineProperty(window, 'api', {
    configurable: true,
    value: { shell: { openUrl: openUrlMock } }
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('TabGroupEmptyState', () => {
  it('renders every detected agent unselected and disables the workspace action', () => {
    renderEmptyState()

    expect(screen.getByText('Claude')).toBeInTheDocument()
    expect(screen.getByText('Codex')).toBeInTheDocument()
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('0')
    expect(required('[data-agent-count="codex"]')).toHaveTextContent('0')
    expect(required<HTMLButtonElement>('[data-start-workspace]')).toBeDisabled()
    // Undetected featured agents render as install entries, not selectable rows.
    expect(required('[data-agent-install-row="opencode"]')).toBeInTheDocument()
    expect(document.querySelector('[data-agent-row="opencode"]')).toBeNull()
  })

  it('offers selectable workspace templates', () => {
    renderEmptyState()

    expect(required('[data-workspace-template="research"]')).toHaveAttribute('aria-pressed', 'true')
    fireEvent.click(required('[data-workspace-template="build"]'))
    expect(required('[data-workspace-template="build"]')).toHaveAttribute('aria-pressed', 'true')
    expect(useTemplateMock).not.toHaveBeenCalled()
  })

  it('paints template feedback before opening its panes', async () => {
    renderEmptyState()

    fireEvent.click(required('[data-use-workspace-template="review"]'))

    expect(useTemplateMock).not.toHaveBeenCalled()
    await flushNextAnimationFrame()
    expect(useTemplateMock).not.toHaveBeenCalled()
    await flushNextAnimationFrame()

    await vi.waitFor(() => expect(useTemplateMock).toHaveBeenCalledWith('review', 'claude'))
    expect(required('[data-workspace-template="review"]')).toHaveAttribute('aria-pressed', 'true')
  })

  it('does not launch an agent when its identity is clicked', () => {
    renderEmptyState()

    fireEvent.click(required('[data-agent-row="claude"]'))

    expect(launchMock).not.toHaveBeenCalled()
  })

  it('paints launch feedback before opening selected sessions in a balanced pane grid', async () => {
    const { activated } = renderEmptyState()

    fireEvent.click(required('[data-agent-count-increment="claude"]'))
    fireEvent.click(required('[data-agent-count-increment="claude"]'))
    fireEvent.click(required('[data-agent-count-increment="codex"]'))
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('2')
    expect(required('[data-agent-count="codex"]')).toHaveTextContent('1')

    fireEvent.click(required('[data-start-workspace]'))

    expect(launchMock).not.toHaveBeenCalled()
    expect(required('[data-start-workspace]')).toHaveAttribute('aria-busy', 'true')
    expect(queuedAnimationFrames).toHaveLength(1)
    await flushNextAnimationFrame()
    expect(launchMock).not.toHaveBeenCalled()
    await flushNextAnimationFrame()

    await vi.waitFor(() => expect(launchMock).toHaveBeenCalledTimes(2))
    expect(queuedAnimationFrames).toHaveLength(1)
    await flushNextAnimationFrame()
    await vi.waitFor(() => expect(launchMock).toHaveBeenCalledTimes(3))
    expect(createEmptyGridGroupsMock).toHaveBeenCalledWith('wt-1', 'group-1', [
      'claude',
      'claude',
      'codex'
    ])
    expect(clearPendingWorkspaceLaunchGroupsMock.mock.calls).toEqual([
      [['group-1', 'group-2']],
      [['group-3']]
    ])
    expect(launchedGroupIds()).toEqual(['group-1', 'group-2', 'group-3'])
    expect(activated).toEqual(['tab-3'])
    expect(launchedAgents()).toEqual(['claude', 'claude', 'codex'])
  })

  it('clamps the session count to 0..10', () => {
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
    expect(required('[data-agent-count="claude"]')).toHaveTextContent('0')
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
      expect(document.querySelector('[data-agent-row="claude"]')).toBeNull()
    } finally {
      detectedIdsBox.value = ['claude', 'codex']
    }
  })
})
