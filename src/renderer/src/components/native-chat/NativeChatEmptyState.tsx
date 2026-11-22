import { Loader2, LogIn, MessageSquare, TriangleAlert } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import { agentTypeToIconAgent, formatAgentTypeLabel } from '@/lib/agent-status'
import { AgentIcon } from '@/lib/agent-catalog'
import { NATIVE_CHAT_EMPTY_STATE_COPY } from '../../../../shared/native-chat-empty-state'
import type { NativeChatSession } from '../../../../shared/native-chat-types'

export function NativeChatEmptyState({
  kind,
  message,
  agent,
  actionLabel,
  onAction,
  secondaryActionLabel,
  onSecondaryAction
}: {
  kind: 'loading' | 'empty' | 'error' | 'not-agent' | 'auth-required'
  message?: string
  agent?: NativeChatSession['agent']
  actionLabel?: string
  onAction?: () => void
  secondaryActionLabel?: string
  onSecondaryAction?: () => void
}): React.JSX.Element {
  const copy = emptyStateCopy(kind, message, agent)
  return (
    <div
      role={
        kind === 'error' || kind === 'auth-required'
          ? 'alert'
          : kind === 'loading'
            ? 'status'
            : undefined
      }
      aria-live={
        kind === 'error' || kind === 'auth-required'
          ? 'assertive'
          : kind === 'loading'
            ? 'polite'
            : undefined
      }
      aria-busy={kind === 'loading' || undefined}
      data-native-chat-empty-state={kind}
      className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center"
    >
      <div
        data-native-chat-empty-icon
        className={
          kind === 'error'
            ? 'flex size-12 items-center justify-center rounded-full bg-destructive/10 text-destructive'
            : kind === 'auth-required'
              ? 'flex size-12 items-center justify-center rounded-full bg-agent-question/10 text-agent-question'
              : 'flex size-12 items-center justify-center rounded-full bg-accent text-accent-foreground'
        }
      >
        {kind === 'loading' ? (
          <Loader2 className="size-6 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        ) : kind === 'error' ? (
          <TriangleAlert className="size-6" />
        ) : kind === 'auth-required' ? (
          <LogIn className="size-6" />
        ) : kind === 'empty' && agent ? (
          // Why: the brand mark makes "Start a chat with X" instant — the
          // generic speech bubble read as a placeholder, not an identity.
          <AgentIcon agent={agentTypeToIconAgent(agent)} size={26} />
        ) : (
          <MessageSquare className="size-6" />
        )}
      </div>
      <p className="text-sm font-medium text-foreground">{copy.title}</p>
      {copy.subtitle ? (
        <p className="max-w-sm text-balance text-xs text-muted-foreground">{copy.subtitle}</p>
      ) : null}
      {kind === 'empty' ? (
        <p data-native-chat-empty-shortcuts className="text-xs text-foreground/70">
          {translate(
            'components.native-chat.state.empty.shortcuts',
            'Use @ for workspace files or / for commands.'
          )}
        </p>
      ) : null}
      {actionLabel && onAction ? (
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button type="button" size="sm" onClick={onAction}>
            {actionLabel}
          </Button>
          {secondaryActionLabel && onSecondaryAction ? (
            <Button type="button" size="sm" variant="outline" onClick={onSecondaryAction}>
              {secondaryActionLabel}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

function emptyStateCopy(
  kind: 'loading' | 'empty' | 'error' | 'not-agent' | 'auth-required',
  message?: string,
  agent?: NativeChatSession['agent']
): { title: string; subtitle: string | null } {
  switch (kind) {
    case 'loading':
      return {
        title: translate(
          'components.native-chat.state.loading.title',
          NATIVE_CHAT_EMPTY_STATE_COPY.loading.title
        ),
        subtitle: translate(
          'components.native-chat.state.loading.subtitle',
          NATIVE_CHAT_EMPTY_STATE_COPY.loading.subtitle
        )
      }
    case 'error':
      return {
        title: translate(
          'components.native-chat.state.error.title',
          NATIVE_CHAT_EMPTY_STATE_COPY.error.title
        ),
        subtitle:
          message ??
          translate(
            'components.native-chat.state.error.subtitle',
            NATIVE_CHAT_EMPTY_STATE_COPY.error.subtitle
          )
      }
    case 'not-agent':
      return {
        title: translate(
          'components.native-chat.state.notAgent.title',
          NATIVE_CHAT_EMPTY_STATE_COPY.notAgent.title
        ),
        subtitle: translate(
          'components.native-chat.state.notAgent.subtitle',
          NATIVE_CHAT_EMPTY_STATE_COPY.notAgent.subtitle
        )
      }
    case 'auth-required': {
      const agentName = agent ? formatAgentTypeLabel(agent) : 'The agent'
      return {
        title: translate(
          'components.native-chat.state.authRequired.title',
          NATIVE_CHAT_EMPTY_STATE_COPY.authRequired.title,
          { value0: agentName }
        ),
        subtitle: translate(
          'components.native-chat.state.authRequired.subtitle',
          NATIVE_CHAT_EMPTY_STATE_COPY.authRequired.subtitle
        )
      }
    }
    case 'empty': {
      const agentName = agent ? formatAgentTypeLabel(agent) : 'the agent'
      return {
        title: translate(
          'components.native-chat.state.empty.title',
          NATIVE_CHAT_EMPTY_STATE_COPY.empty.title,
          { value0: agentName }
        ),
        subtitle: translate(
          'components.native-chat.state.empty.subtitle',
          NATIVE_CHAT_EMPTY_STATE_COPY.empty.subtitle,
          { value0: agentName }
        )
      }
    }
  }
}
