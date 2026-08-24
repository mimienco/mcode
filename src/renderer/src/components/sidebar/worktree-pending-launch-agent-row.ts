import type { DashboardAgentRow } from '@/components/dashboard/useDashboardData'
import { collectLeafIdsInOrder } from '@/components/terminal-pane/terminal-layout-leaf-ids'
import { formatAgentTypeLabel } from '@/lib/agent-status'
import type { AgentStatusEntry } from '../../../../shared/agent-status-types'
import { isTerminalLeafId, makePaneKey } from '../../../../shared/stable-pane-id'
import type {
  TerminalLayoutSnapshot,
  TerminalTab
} from '../../../../shared/terminal-tab-types'

/** Pending-launch rows are a projection of tab intent, not a sequenced process observation. */
export const TITLE_DERIVED_AGENT_ROW_AUTHORITY_ID = 'renderer-title-projection'

/**
 * Represents an agent tab that is already visible but whose first PTY has not
 * mounted yet. The launch intent is authoritative only for a single-leaf tab;
 * split panes must wait for pane-scoped identity evidence.
 */
export function buildPendingLaunchAgentRow(args: {
  tab: TerminalTab
  layout: TerminalLayoutSnapshot | undefined
  seenPaneKeys: Set<string>
  now: number
}): DashboardAgentRow | null {
  if (!args.tab.launchAgent || args.tab.ptyId !== null) {
    return null
  }
  const leafIds = collectLeafIdsInOrder(args.layout?.root)
  const leafId = leafIds[0]
  if (leafIds.length !== 1 || !leafId || !isTerminalLeafId(leafId)) {
    return null
  }
  const paneKey = makePaneKey(args.tab.id, leafId)
  if (args.seenPaneKeys.has(paneKey)) {
    return null
  }
  const agentType = args.tab.launchAgent
  const label = formatAgentTypeLabel(agentType)
  const entry: AgentStatusEntry = {
    paneKey,
    state: 'working',
    prompt: label,
    updatedAt: args.now,
    stateStartedAt: 0,
    stateHistory: [],
    agentType,
    terminalTitle: args.tab.title,
    lastAssistantMessage: 'Not started',
    observation: {
      origin: 'title',
      authorityId: TITLE_DERIVED_AGENT_ROW_AUTHORITY_ID,
      incarnation: 0,
      revision: args.now,
      observedAt: args.now,
      kind: 'snapshot'
    }
  }
  return {
    paneKey,
    entry,
    tab: args.tab,
    agentType,
    rowSource: 'live',
    state: 'idle',
    startedAt: 0
  }
}
