import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { AGENT_STATUS_STATES } from './agent-status-types'
import type { AiVaultSessionTitle } from './ai-vault-session-title'
import type { AgentProviderSessionKey, SleepingAgentSessionRecord } from './agent-session-resume'
import { RESUMABLE_TUI_AGENTS } from './agent-session-resume'
import type { BrowserViewportPresetId } from './browser-workspace-types'
import { parseExecutionHostId } from './execution-host'
import { SHUTDOWN_CHECKPOINT_VERSION } from './shutdown-checkpoint-types'
import type {
  Tab,
  TabContentType,
  TabGroupSplitDirection,
  WorkspaceVisibleTabType
} from './tab-types'
import type { TerminalPaneSplitDirection } from './terminal-tab-types'
import type { TuiAgent } from './tui-agent'
import type { TopLevelView } from './ui-chrome-types'
import { parseWorkspaceSession } from './workspace-session-schema'
import type { WorkspaceSessionState } from './workspace-session-state-types'

function exhaustiveValues<Domain extends PropertyKey>() {
  return <const Values extends readonly Domain[]>(
    values: Values &
      (Exclude<Domain, Values[number]> extends never
        ? unknown
        : ['Missing values', Exclude<Domain, Values[number]>])
  ): Values => values
}

const WORKSPACE_SESSION_STATE_KEYS = exhaustiveValues<keyof WorkspaceSessionState>()([
  'activeRepoId',
  'activeWorkspaceKey',
  'activeWorkspaceExecutionHostId',
  'activeWorktreeId',
  'activeTabId',
  'tabsByWorktree',
  'terminalLayoutsByTabId',
  'activeWorktreeIdsOnShutdown',
  'openFilesByWorktree',
  'activeFileIdByWorktree',
  'markdownFrontmatterVisible',
  'browserTabsByWorktree',
  'browserPagesByWorkspace',
  'activeBrowserTabIdByWorktree',
  'activeTabTypeByWorktree',
  'browserUrlHistory',
  'activeTabIdByWorktree',
  'unifiedTabs',
  'tabGroups',
  'tabGroupLayouts',
  'activeGroupIdByWorktree',
  'activeConnectionIdsAtShutdown',
  'remoteSessionIdsByTabId',
  'lastVisitedAtByWorktreeId',
  'defaultTerminalTabsAppliedByWorktreeId',
  'sleepingAgentSessionsByPaneKey',
  'terminalPtyIncarnationsByPaneKey',
  'terminalTopologyRevisionByRepoId',
  'terminalSurfaceTombstonesByPaneKey'
])

const TOP_LEVEL_VIEWS = exhaustiveValues<TopLevelView>()([
  'terminal',
  'settings',
  'tasks',
  'activity',
  'automations',
  'space',
  'skills',
  'artifacts',
  'mobile'
])

const TUI_AGENTS = exhaustiveValues<TuiAgent>()([
  'claude',
  'claude-agent-teams',
  'openclaude',
  'codex',
  'autohand',
  'opencode',
  'mimo-code',
  'pi',
  'omp',
  'gemini',
  'antigravity',
  'aider',
  'goose',
  'amp',
  'kilo',
  'kiro',
  'crush',
  'aug',
  'cline',
  'codebuff',
  'command-code',
  'continue',
  'cursor',
  'droid',
  'kimi',
  'mistral-vibe',
  'qwen-code',
  'rovo',
  'hermes',
  'openclaw',
  'copilot',
  'grok',
  'devin',
  'ante',
  'trae',
  'prime-agent'
])

const TERMINAL_PANE_SPLIT_DIRECTIONS = exhaustiveValues<TerminalPaneSplitDirection>()([
  'vertical',
  'horizontal'
])
const TAB_GROUP_SPLIT_DIRECTIONS = exhaustiveValues<TabGroupSplitDirection>()([
  'horizontal',
  'vertical'
])
const TAB_CONTENT_TYPES = exhaustiveValues<TabContentType>()([
  'terminal',
  'editor',
  'diff',
  'conflict-review',
  'check-details',
  'browser',
  'simulator'
])
const WORKSPACE_VISIBLE_TAB_TYPES = exhaustiveValues<WorkspaceVisibleTabType>()([
  'terminal',
  'editor',
  'browser',
  'simulator'
])
const TAB_VIEW_MODES = exhaustiveValues<NonNullable<Tab['viewMode']>>()(['terminal', 'chat'])
const AI_VAULT_TITLE_AGENTS = exhaustiveValues<AiVaultSessionTitle['agent']>()(['claude', 'codex'])
const BROWSER_VIEWPORT_PRESET_IDS = exhaustiveValues<BrowserViewportPresetId>()([
  'mobile-s',
  'mobile-m',
  'mobile-l',
  'tablet',
  'laptop',
  'laptop-l',
  'desktop'
])
const PROVIDER_SESSION_KEYS = exhaustiveValues<AgentProviderSessionKey>()([
  'session_id',
  'conversation_id'
])
const SLEEPING_AGENT_ORIGINS = exhaustiveValues<
  NonNullable<SleepingAgentSessionRecord['origin']>
>()(['worktree-sleep', 'quit', 'live'])
const AUTOMATIC_RESUME_BLOCKED_BY_VALUES = exhaustiveValues<
  NonNullable<SleepingAgentSessionRecord['automaticResumeBlockedBy']>
>()(['legacy-orchestration-worker'])

type ShutdownCheckpointFixture = {
  checkpoint: {
    version: number
    sessions: { state: unknown; hostId?: string }[]
    ui: { activeView?: string }
  }
  workspaceSessionStateKeys: string[]
  topLevelViews: string[]
  tuiAgents: string[]
  resumableTuiAgents: string[]
  terminalPaneSplitDirections: string[]
  tabGroupSplitDirections: string[]
  tabContentTypes: string[]
  workspaceVisibleTabTypes: string[]
  tabViewModes: string[]
  aiVaultTitleAgents: string[]
  browserViewportPresetIds: string[]
  providerSessionKeys: string[]
  sleepingAgentStates: string[]
  sleepingAgentOrigins: string[]
  automaticResumeBlockedByValues: string[]
  executionHostIds: string[]
}

function readFixture(): ShutdownCheckpointFixture {
  return JSON.parse(
    readFileSync(
      resolve('crates/mcode-settings/tests/fixtures/shutdown-checkpoint-v1.json'),
      'utf8'
    )
  ) as ShutdownCheckpointFixture
}

describe('shutdown checkpoint v1 shared contract', () => {
  it('keeps TypeScript and Rust aligned on every workspace-session field', () => {
    const fixture = readFixture()

    expect(fixture.workspaceSessionStateKeys.toSorted()).toEqual(
      WORKSPACE_SESSION_STATE_KEYS.toSorted()
    )
    expect(Object.keys(fixture.checkpoint.sessions[0]!.state as object).toSorted()).toEqual(
      WORKSPACE_SESSION_STATE_KEYS.toSorted()
    )
    for (const session of fixture.checkpoint.sessions) {
      expect(parseWorkspaceSession(session.state)).toMatchObject({ ok: true })
    }
  })

  it('keeps versioned UI views and execution-host identities aligned', () => {
    const fixture = readFixture()

    expect(fixture.checkpoint.version).toBe(SHUTDOWN_CHECKPOINT_VERSION)
    expect(fixture.topLevelViews).toEqual(TOP_LEVEL_VIEWS)
    expect(fixture.tuiAgents).toEqual(TUI_AGENTS)
    expect(fixture.resumableTuiAgents).toEqual(RESUMABLE_TUI_AGENTS)
    expect(fixture.terminalPaneSplitDirections).toEqual(TERMINAL_PANE_SPLIT_DIRECTIONS)
    expect(fixture.tabGroupSplitDirections).toEqual(TAB_GROUP_SPLIT_DIRECTIONS)
    expect(fixture.tabContentTypes).toEqual(TAB_CONTENT_TYPES)
    expect(fixture.workspaceVisibleTabTypes).toEqual(WORKSPACE_VISIBLE_TAB_TYPES)
    expect(fixture.tabViewModes).toEqual(TAB_VIEW_MODES)
    expect(fixture.aiVaultTitleAgents).toEqual(AI_VAULT_TITLE_AGENTS)
    expect(fixture.browserViewportPresetIds).toEqual(BROWSER_VIEWPORT_PRESET_IDS)
    expect(fixture.providerSessionKeys).toEqual(PROVIDER_SESSION_KEYS)
    expect(fixture.sleepingAgentStates).toEqual(AGENT_STATUS_STATES)
    expect(fixture.sleepingAgentOrigins).toEqual(SLEEPING_AGENT_ORIGINS)
    expect(fixture.automaticResumeBlockedByValues).toEqual(AUTOMATIC_RESUME_BLOCKED_BY_VALUES)
    expect(fixture.executionHostIds.map(parseExecutionHostId).every(Boolean)).toBe(true)
  })
})
