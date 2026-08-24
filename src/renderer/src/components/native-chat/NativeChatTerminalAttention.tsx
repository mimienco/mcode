import type { ReactNode } from 'react'
import { Copy, LogIn, ShieldAlert } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import { formatAgentTypeLabel } from '@/lib/agent-status'
import type { AgentType } from '../../../../shared/agent-status-types'
import type { NativeChatTerminalAttention as TerminalAttention } from './native-chat-terminal-attention'

export function NativeChatTerminalAttention({
  attention,
  agent,
  onSwitchToTerminal,
  children
}: {
  attention: TerminalAttention | null
  agent: AgentType
  onSwitchToTerminal?: () => void
  children: ReactNode
}): React.JSX.Element {
  if (!attention) {
    return <>{children}</>
  }

  const authRequired = attention === 'agent-auth-required'
  const title = authRequired
    ? translate(
        'components.native-chat.terminalAttention.auth.title',
        '{{value0}} session expired',
        { value0: formatAgentTypeLabel(agent) }
      )
    : translate(
        'components.native-chat.terminalAttention.hooksReview.title',
        'Codex needs a security review'
      )
  const description = authRequired
    ? translate(
        'components.native-chat.terminalAttention.auth.description',
        'Log in again to continue. Your draft is preserved.'
      )
    : translate(
        'components.native-chat.terminalAttention.hooksReview.description',
        'Review new or changed hooks before Chat can continue.'
      )
  const action = authRequired
    ? translate('components.native-chat.terminalAttention.auth.action', 'Log in in Terminal')
    : translate('components.native-chat.terminalAttention.hooksReview.action', 'Review in Terminal')

  return (
    <>
      <div
        role="alert"
        className="mx-3 mb-3 flex flex-wrap items-center gap-3 rounded-md border border-agent-question/30 bg-agent-question/[0.06] px-3 py-2.5 sm:mx-5"
      >
        {authRequired ? (
          <LogIn className="size-4 shrink-0 text-agent-question" aria-hidden="true" />
        ) : (
          <ShieldAlert className="size-4 shrink-0 text-agent-question" aria-hidden="true" />
        )}
        <div className="min-w-48 flex-1">
          <p className="text-sm font-medium text-foreground">{title}</p>
          <p className="text-xs leading-relaxed text-foreground/75">{description}</p>
        </div>
        {authRequired ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="shrink-0 font-mono"
            onClick={() => {
              void window.api.ui.writeClipboardText('/login').then(
                () =>
                  toast.success(
                    translate(
                      'components.native-chat.terminalAttention.auth.copied',
                      'Copied /login'
                    )
                  ),
                () =>
                  toast.error(
                    translate(
                      'components.native-chat.terminalAttention.auth.copyFailed',
                      'Could not copy /login'
                    )
                  )
              )
            }}
          >
            <Copy className="size-3.5" />
            /login
          </Button>
        ) : null}
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={!onSwitchToTerminal}
          className="shrink-0"
          onClick={onSwitchToTerminal}
        >
          {action}
        </Button>
      </div>
      <div className="hidden" aria-hidden="true">
        {children}
      </div>
    </>
  )
}
