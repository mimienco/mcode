import type { TuiAgent } from '../../../../shared/tui-agent'

export function nativeChatSessionIsTranscriptOnly(args: {
  persistedSession: { agent: Extract<TuiAgent, 'claude' | 'codex'>; sessionId: string } | null
  hasLiveAgentEvidence: boolean
}): boolean {
  return args.persistedSession !== null && !args.hasLiveAgentEvidence
}
