import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { parseExecutionHostId } from './execution-host'
import { RESUMABLE_TUI_AGENTS } from './agent-session-resume'
import { SHUTDOWN_CHECKPOINT_VERSION } from './shutdown-checkpoint-types'
import type { TuiAgent } from './tui-agent'
import type { TopLevelView } from './ui-chrome-types'
import { parseWorkspaceSession } from './workspace-session-schema'
import type { WorkspaceSessionState } from './workspace-session-state-types'

const WORKSPACE_SESSION_STATE_KEYS = [
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
] as const satisfies readonly (keyof WorkspaceSessionState)[]

const TOP_LEVEL_VIEWS = [
  'terminal',
  'settings',
  'tasks',
  'activity',
  'automations',
  'space',
  'skills',
  'artifacts',
  'mobile'
] as const satisfies readonly TopLevelView[]

const TUI_AGENTS = [
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
] as const satisfies readonly TuiAgent[]

type MissingWorkspaceSessionStateKey = Exclude<
  keyof WorkspaceSessionState,
  (typeof WORKSPACE_SESSION_STATE_KEYS)[number]
>
type MissingTopLevelView = Exclude<TopLevelView, (typeof TOP_LEVEL_VIEWS)[number]>
type MissingTuiAgent = Exclude<TuiAgent, (typeof TUI_AGENTS)[number]>

const workspaceSessionStateKeysAreExhaustive: MissingWorkspaceSessionStateKey extends never
  ? true
  : false = true
const topLevelViewsAreExhaustive: MissingTopLevelView extends never ? true : false = true
const tuiAgentsAreExhaustive: MissingTuiAgent extends never ? true : false = true

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

    expect(workspaceSessionStateKeysAreExhaustive).toBe(true)
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
    expect(topLevelViewsAreExhaustive).toBe(true)
    expect(fixture.topLevelViews).toEqual(TOP_LEVEL_VIEWS)
    expect(tuiAgentsAreExhaustive).toBe(true)
    expect(fixture.tuiAgents).toEqual(TUI_AGENTS)
    expect(fixture.resumableTuiAgents).toEqual(RESUMABLE_TUI_AGENTS)
    expect(fixture.executionHostIds.map(parseExecutionHostId).every(Boolean)).toBe(true)
  })
})
