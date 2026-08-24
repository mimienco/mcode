import React from 'react'
import { Replace, CaseSensitive, Loader2, ArrowRightToLine } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ToggleButton } from './SearchResultItems'
import { translate } from '@/i18n/i18n'

export type SearchReplaceRowProps = {
  value: string
  preserveCase: 'none' | 'match'
  useRegex: boolean
  running: boolean
  hasResults: boolean
  confirmOccurrences: number | null
  onValueChange: (value: string) => void
  onPreserveCaseToggle: () => void
  onRequestReplace: () => void
  onConfirmReplace: () => void
  onCancelConfirm: () => void
}

export function SearchReplaceRow({
  value,
  preserveCase,
  useRegex,
  running,
  hasResults,
  confirmOccurrences,
  onValueChange,
  onPreserveCaseToggle,
  onRequestReplace,
  onConfirmReplace,
  onCancelConfirm
}: SearchReplaceRowProps): React.JSX.Element {
  if (confirmOccurrences !== null) {
    return (
      <div
        className="flex h-7 items-center gap-1.5 rounded-sm border border-ring/60 bg-background px-1.5"
        role="group"
        aria-label={translate(
          'auto.components.right.sidebar.replace.confirm.root',
          'Replace {{value0}} occurrence(s)?',
          { value0: confirmOccurrences }
        )}
      >
        <span className="truncate text-xs text-foreground">
          {translate(
            'auto.components.right.sidebar.replace.confirm.root',
            'Replace {{value0}} occurrence(s)?',
            { value0: confirmOccurrences }
          )}
        </span>
        <Button
          size="sm"
          className="ml-auto h-5 shrink-0 px-2 text-[11px] font-medium"
          onClick={onConfirmReplace}
          autoFocus
        >
          {translate('auto.components.right.sidebar.replace.confirm.yes', 'Replace')}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          className="h-5 shrink-0 px-2 text-[11px]"
          onClick={onCancelConfirm}
        >
          {translate('auto.components.right.sidebar.replace.confirm.no', 'Cancel')}
        </Button>
      </div>
    )
  }

  return (
    <div
      className="flex h-7 items-center gap-1 rounded-sm border border-border bg-input/50 px-1.5 focus-within:border-ring"
      data-ignore-file-explorer-keys="true"
    >
      <Replace className="size-3.5 shrink-0 text-muted-foreground" />
      <input
        className="h-full w-full bg-transparent text-xs outline-none placeholder:text-muted-foreground"
        value={value}
        disabled={running}
        placeholder={translate('auto.components.right.sidebar.replace.placeholder', 'Replace')}
        onChange={(e) => onValueChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
            onRequestReplace()
          }
        }}
      />
      {running ? <Loader2 className="size-3 shrink-0 animate-spin text-muted-foreground" /> : null}
      {useRegex ? null : (
        <ToggleButton
          title={translate('auto.components.right.sidebar.replace.preserveCase', 'Preserve case')}
          active={preserveCase === 'match'}
          onClick={onPreserveCaseToggle}
        >
          <CaseSensitive className="size-3.5" />
        </ToggleButton>
      )}
      <Button
        size="sm"
        className="h-5 shrink-0 gap-0.5 px-1.5 text-[11px]"
        disabled={!hasResults || running || value.length === 0}
        onClick={onRequestReplace}
      >
        <ArrowRightToLine className="size-3" />
        {translate('auto.components.right.sidebar.replace.all', 'Replace All')}
      </Button>
    </div>
  )
}
