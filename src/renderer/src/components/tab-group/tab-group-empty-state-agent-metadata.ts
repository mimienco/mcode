import { getAgentCatalog } from '@/lib/agent-catalog'
import { translate } from '@/i18n/i18n'
import type { TuiAgent } from '../../../../shared/tui-agent'

const AGENT_CHANNEL_COLOR: Record<string, string> = {
  claude: 'var(--channel-claude)',
  'claude-agent-teams': 'var(--channel-claude)',
  openclaude: 'var(--channel-claude)',
  codex: 'var(--channel-codex)',
  grok: 'var(--channel-grok)',
  pi: 'var(--channel-pi)',
  omp: 'var(--channel-omp)'
}

export function getEmptyStateAgentChannelColor(agent: TuiAgent): string {
  return AGENT_CHANNEL_COLOR[agent] ?? 'var(--muted-foreground)'
}

export function getEmptyStateAgentLabel(agent: TuiAgent): string {
  if (agent === 'kimi') {
    return translate('auto.components.tab.bar.QuickLaunchButton.kimiCodeCliLabel', 'Kimi Code CLI')
  }
  return getAgentCatalog().find((entry) => entry.id === agent)?.label ?? agent
}
