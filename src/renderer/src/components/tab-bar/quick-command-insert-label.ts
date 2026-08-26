import type { HostedTerminalQuickCommand } from '@/hooks/use-terminal-quick-command-hosts'
import { getAgentLabel } from '@/lib/agent-catalog'
import {
  getTerminalQuickCommandBody,
  isTerminalAgentQuickCommand
} from '../../../../shared/terminal-quick-commands'

export function getQuickCommandInsertDescription(
  entry: HostedTerminalQuickCommand,
  insertVerb: string
): string {
  const agentPrefix = isTerminalAgentQuickCommand(entry.command)
    ? `${getAgentLabel(entry.command.agent)}: `
    : ''
  return `${insertVerb} ${agentPrefix}${getTerminalQuickCommandBody(entry.command)}`
}
