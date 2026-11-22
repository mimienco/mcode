import { ArrowUpCircle, CheckCircle2, ExternalLink } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import type { NativeChatTerminalNotice as TerminalNotice } from './native-chat-terminal-attention'

export function NativeChatTerminalNotice({
  notice,
  onUpdate
}: {
  notice: TerminalNotice | null
  onUpdate?: (command: string) => void
}): React.JSX.Element | null {
  if (!notice) {
    return null
  }
  const complete = notice.kind === 'update-complete'
  const updateCommand = notice.kind === 'update-available' ? notice.command : null
  return (
    <div
      role="status"
      aria-live="polite"
      className={
        complete
          ? 'mx-3 mt-3 flex flex-wrap items-center gap-3 rounded-md border border-agent-success/30 bg-agent-success/[0.06] px-3 py-2.5 sm:mx-5'
          : 'mx-3 mt-3 flex flex-wrap items-center gap-3 rounded-md border border-primary/30 bg-primary/[0.06] px-3 py-2.5 sm:mx-5'
      }
    >
      {complete ? (
        <CheckCircle2 className="size-4 shrink-0 text-agent-success" aria-hidden="true" />
      ) : (
        <ArrowUpCircle className="size-4 shrink-0 text-primary" aria-hidden="true" />
      )}
      <div className="min-w-48 flex-1">
        <p className="text-sm font-medium text-foreground">
          {complete
            ? translate(
                'components.native-chat.terminalNotice.updateComplete.title',
                'Claude Code updated'
              )
            : translate(
                'components.native-chat.terminalNotice.updateAvailable.title',
                'Claude Code update available'
              )}
        </p>
        <p className="text-xs leading-relaxed text-foreground/75">
          {complete
            ? translate(
                'components.native-chat.terminalNotice.updateComplete.description',
                '{{value0}} features, {{value1}} bugfixes, and {{value2}} other changes.',
                {
                  value0: notice.features,
                  value1: notice.bugfixes,
                  value2: notice.otherChanges
                }
              )
            : (notice.command ??
              translate(
                'components.native-chat.terminalNotice.updateAvailable.manual',
                'Open Terminal to use the package-manager update command.'
              ))}
        </p>
      </div>
      {complete ? (
        <Button asChild size="sm" variant="ghost" className="shrink-0">
          <a href={notice.changelogUrl} target="_blank" rel="noreferrer">
            {translate(
              'components.native-chat.terminalNotice.updateComplete.changelog',
              'Changelog'
            )}
            <ExternalLink className="size-3.5" />
          </a>
        </Button>
      ) : updateCommand && onUpdate ? (
        <Button
          type="button"
          size="sm"
          className="shrink-0"
          onClick={() => onUpdate(updateCommand)}
        >
          {translate(
            'components.native-chat.terminalNotice.updateAvailable.action',
            'Update Claude'
          )}
        </Button>
      ) : null}
    </div>
  )
}
