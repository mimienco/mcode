import type {
  CatalogAgentInteractionDetection,
  CatalogCommandDelivery
} from '../../../../shared/agent-session-option-catalog'
import type { ClaudeModelSwitchOutcome } from './claude-model-switch-confirmation'
import type { CodexLiveSessionOptionsUpdate } from '../../../../shared/codex-live-session-options'

export type NativeChatSessionOptionDispatchResult = {
  outcome?: ClaudeModelSwitchOutcome
  appliedValues?: { model?: string; effort?: string }
}

export type NativeChatSessionOptionDispatchCommand = (
  command: string,
  options?: {
    detectAgentInteraction?: CatalogAgentInteractionDetection
    expectedChoiceLabel?: string
    delivery?: CatalogCommandDelivery
    codexSessionOptions?: CodexLiveSessionOptionsUpdate
  }
) =>
  | Promise<NativeChatSessionOptionDispatchResult | void>
  | NativeChatSessionOptionDispatchResult
  | void
