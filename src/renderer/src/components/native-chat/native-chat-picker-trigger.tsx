import { ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { translate } from '@/i18n/i18n'

function PickerTooltipContent(props: {
  label: string
  disabledReason?: string | null
  dispatched: boolean
}): React.JSX.Element {
  return (
    <div className="space-y-0.5">
      <div>{props.disabledReason ?? props.label}</div>
      {props.dispatched ? (
        <div>
          {translate(
            'components.native-chat.composer.sentNotConfirmed',
            'Sent to the agent — not confirmed'
          )}
        </div>
      ) : null}
    </div>
  )
}

export function PickerTrigger(props: {
  label: string
  tooltipLabel: string
  disabled: boolean
  disabledReason?: string | null
  dispatched: boolean
  /** Extra classes for the value slot; the model pill sets mono readout type. */
  valueClassName?: string
}): React.JSX.Element {
  // Why: value-only visible text must still include the category in the
  // accessible name (WCAG 2.5.3 Label in Name / voice control).
  const accessibleName =
    props.label === props.tooltipLabel
      ? props.tooltipLabel
      : translate('components.native-chat.composer.pillAccessibleName', '{{value0}} {{value1}}', {
          value0: props.tooltipLabel,
          value1: props.label
        })
  const hasValue = props.label !== props.tooltipLabel
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {/* Keep tooltip and dropdown on separate DOM nodes. Nesting both
            Radix triggers onto the button lets the tooltip slot overwrite the
            menu trigger's pointer handlers and aria-haspopup contract. */}
        <span className="inline-flex">
          <DropdownMenuTrigger asChild disabled={props.disabled}>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label={accessibleName}
              aria-description={props.disabledReason ?? undefined}
              className="max-w-48 gap-1.5 text-muted-foreground transition-colors hover:text-foreground pointer-coarse:h-11"
            >
              {hasValue ? (
                <>
                  {/* Category stays tiny and muted; the value carries the weight,
                      so "Model gpt-5.6-sol" scans as one fact, not two buttons. */}
                  <span
                    data-native-chat-picker-category
                    className="shrink-0 text-[11px] text-muted-foreground/80"
                  >
                    {props.tooltipLabel}
                  </span>
                  <span
                    className={`truncate text-xs font-medium text-foreground ${props.valueClassName ?? ''}`}
                  >
                    {props.label}
                  </span>
                </>
              ) : (
                <span className="truncate">{props.label}</span>
              )}
              <ChevronDown className="size-3 shrink-0 opacity-60" />
            </Button>
          </DropdownMenuTrigger>
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={4}>
        <PickerTooltipContent
          label={props.tooltipLabel}
          disabledReason={props.disabledReason}
          dispatched={props.dispatched}
        />
      </TooltipContent>
    </Tooltip>
  )
}
