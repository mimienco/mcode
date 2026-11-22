import { ShieldQuestion } from 'lucide-react'
import { cn } from '@/lib/utils'
import { translate } from '@/i18n/i18n'
import type { ChatApproval } from './native-chat-interactive-prompt'

export type NativeChatApprovalCardProps = {
  approval: ChatApproval
  isSubmitting: boolean
  deliveryFailed: boolean
  /** Send the chosen option's literal string to the agent's PTY. */
  onChoose: (send: string) => void
}

/**
 * Native renderer for an agent tool-approval (PermissionRequest) as an
 * Allow/Deny card. Each button writes its option's literal `send` string back
 * to the agent (a number to allow; ESC to deny). Styling follows semantic intent
 * instead of option order so a new protocol choice cannot become primary by accident.
 */
export function NativeChatApprovalCard({
  approval,
  isSubmitting,
  deliveryFailed,
  onChoose
}: NativeChatApprovalCardProps): React.JSX.Element {
  return (
    <div className="shrink-0 bg-background">
      <div className="mx-auto w-full max-w-3xl px-3 pt-2 pb-1 sm:px-5 @5xl/native-chat:max-w-4xl">
        <div
          role="region"
          aria-label={translate(
            'components.native-chat.approval.regionLabel',
            'Permission request'
          )}
          aria-busy={isSubmitting}
          className="flex w-full flex-col gap-3 rounded-lg border border-agent-question/30 bg-card px-4 py-3 shadow-xs"
        >
          <div className="flex items-start gap-2">
            <ShieldQuestion className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            <div className="min-w-0">
              <p className="text-sm font-semibold text-foreground">{approval.title}</p>
              {approval.detail ? (
                <p className="mt-0.5 break-words font-mono text-xs text-muted-foreground">
                  {approval.detail}
                </p>
              ) : null}
              <p className="mt-1 text-xs text-foreground/70">
                {translate(
                  'components.native-chat.approval.onceDescription',
                  'Approval applies to this request only.'
                )}
              </p>
            </div>
          </div>
          {deliveryFailed ? (
            <p role="alert" className="text-xs text-destructive">
              {translate(
                'components.native-chat.approval.deliveryFailed',
                'The choice was not delivered. Try again or review it in Terminal.'
              )}
            </p>
          ) : null}
          <div className="flex flex-wrap gap-2">
            {approval.options.map((opt) => (
              <button
                key={`${opt.intent}:${opt.send}`}
                type="button"
                disabled={isSubmitting}
                onClick={() => onChoose(opt.send)}
                className={cn(
                  'min-h-8 rounded-md border px-4 py-1.5 text-sm font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-wait disabled:opacity-50',
                  opt.intent === 'allow-once'
                    ? 'border-agent-question/40 bg-agent-question/[0.08] text-foreground hover:bg-agent-question/[0.14]'
                    : 'border-border bg-background text-foreground hover:bg-accent'
                )}
              >
                {isSubmitting && opt.intent === 'allow-once'
                  ? translate('components.native-chat.approval.sending', 'Sending…')
                  : opt.label}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}
