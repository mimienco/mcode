import type { DragEvent } from 'react'
import { Check, Copy, GripVertical, Pencil, Play, Trash2 } from 'lucide-react'
import { CommandItem } from '@/components/ui/command'
import {
  getTerminalQuickCommandBody,
  isTerminalAgentQuickCommand
} from '../../../../shared/terminal-quick-commands'
import { AgentIcon, getAgentLabel } from '@/lib/agent-catalog'
import { translate } from '@/i18n/i18n'
import { useClipboardTextCopyFeedback } from '@/hooks/use-clipboard-text-copy-feedback'
import type { HostedTerminalQuickCommand } from '@/hooks/use-terminal-quick-command-hosts'
import { cn } from '@/lib/utils'
import {
  finishQuickCommandDrag,
  writeQuickCommandDragData
} from '@/lib/quick-command-drag-data'

type TabBarQuickCommandItemProps = {
  entry: HostedTerminalQuickCommand
  showHostLabel: boolean
  onInsert: () => void
  onDragStarted?: () => void
  onDragFinished?: (dropped: boolean) => void
}

export function TabBarQuickCommandItem({
  entry,
  showHostLabel,
  onInsert,
  onDragStarted,
  onDragFinished
}: TabBarQuickCommandItemProps): React.JSX.Element {
  const { command } = entry
  const body = getTerminalQuickCommandBody(command)
  const insertVerb = translate(
    'auto.components.terminal.pane.TerminalContextMenu.c2f0b72b8d',
    'Insert'
  )
  return (
    <CommandItem
      value={entry.key}
      onSelect={onInsert}
      aria-label={`${insertVerb} ${command.label}`}
      draggable
      onDragStart={(event) => {
        if (!writeQuickCommandDragData(event.dataTransfer, command.label, body)) {
          event.preventDefault()
          return
        }
        onDragStarted?.()
      }}
      onDragEnd={(event: DragEvent<HTMLDivElement>) => {
        const handledInternally = finishQuickCommandDrag(event.clientX, event.clientY)
        onDragFinished?.(handledInternally || event.dataTransfer.dropEffect !== 'none')
      }}
      className="mx-1 my-0.5 min-w-0 cursor-grab items-center gap-2 rounded-[7px] px-2 py-1.5 text-[12px] leading-5 active:cursor-grabbing data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground"
    >
      <GripVertical className="size-3 shrink-0 text-muted-foreground/70" aria-hidden="true" />
      {isTerminalAgentQuickCommand(command) ? (
        <span className="shrink-0 text-muted-foreground">
          <AgentIcon agent={command.agent} size={12} />
        </span>
      ) : (
        <Play
          className="size-3 shrink-0 text-muted-foreground"
          fill="currentColor"
          strokeWidth={0}
        />
      )}
      <span className="min-w-0 flex-1">
        <span className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 flex-1 truncate font-medium text-foreground">
            {command.label}
          </span>
          {showHostLabel ? (
            <span className="shrink-0 text-[11px] text-muted-foreground">{entry.hostLabel}</span>
          ) : null}
        </span>
        <span className="block truncate font-mono text-[11px] text-muted-foreground">
          {isTerminalAgentQuickCommand(command)
            ? `${getAgentLabel(command.agent)}: ${command.prompt}`
            : command.command}
        </span>
      </span>
    </CommandItem>
  )
}

export function TabBarQuickCommandActions({
  entry,
  onRun,
  onEdit,
  onDelete
}: {
  entry: HostedTerminalQuickCommand
  onRun: () => void
  onEdit: () => void
  onDelete: () => void
}): React.JSX.Element {
  const { command } = entry
  const body = getTerminalQuickCommandBody(command)
  const { canCopy, copyText, status } = useClipboardTextCopyFeedback(body)
  const runLabel = translate(
    'auto.components.tab.bar.TabBarQuickCommandsButton.37e1bb90ce',
    'Run: {{value0}}',
    { value0: command.label }
  )
  const copyLabel =
    status === 'copied'
      ? translate('auto.components.tab.bar.TabBarQuickCommandsButton.8d525e5f15', 'Copied')
      : status === 'failed'
        ? translate('auto.components.tab.bar.TabBarQuickCommandsButton.53b17a4b1b', "Couldn't copy")
        : canCopy
          ? translate(
              'auto.components.tab.bar.TabBarQuickCommandsButton.a9a564b7e7',
              'Copy {{value0}}',
              { value0: command.label }
            )
          : translate(
              'auto.components.tab.bar.TabBarQuickCommandsButton.69a1441a21',
              'Nothing to copy'
            )

  return (
    <div
      role="toolbar"
      aria-label={translate(
        'auto.components.tab.bar.TabBarQuickCommandsButton.192a4616a5',
        'Quick command actions'
      )}
      className="flex min-w-0 items-center gap-1 border-t border-border/50 px-2 py-1.5"
    >
      <span className="mr-auto min-w-0 truncate text-[11px] text-muted-foreground">
        {command.label}
      </span>
      <button
        type="button"
        onClick={onRun}
        className="flex cursor-pointer items-center gap-1 rounded px-1.5 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
        aria-label={runLabel}
      >
        <Play className="size-3" fill="currentColor" strokeWidth={0} />
        {translate('auto.components.tab.bar.TabBarQuickCommandsButton.runAction', 'Run')}
      </button>
      <button
        type="button"
        disabled={!canCopy}
        onClick={() => void copyText()}
        className={cn(
          'cursor-pointer rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent',
          status === 'copied' && 'text-status-success',
          status === 'failed' && 'text-destructive'
        )}
        aria-label={copyLabel}
        title={copyLabel}
      >
        {status === 'copied' ? <Check className="size-3" /> : <Copy className="size-3" />}
      </button>
      <button
        type="button"
        onClick={onEdit}
        className="cursor-pointer rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        aria-label={translate(
          'auto.components.tab.bar.TabBarQuickCommandsButton.15529ede69',
          'Edit {{value0}}',
          { value0: command.label }
        )}
      >
        <Pencil className="size-3" />
      </button>
      <button
        type="button"
        onClick={onDelete}
        className="cursor-pointer rounded p-1 text-muted-foreground hover:bg-accent hover:text-destructive"
        aria-label={translate(
          'auto.components.tab.bar.TabBarQuickCommandsButton.196593b6a9',
          'Remove {{value0}}',
          { value0: command.label }
        )}
      >
        <Trash2 className="size-3" />
      </button>
    </div>
  )
}
