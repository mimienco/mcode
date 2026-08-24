import {
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator
} from '@/components/ui/dropdown-menu'
import { translate } from '@/i18n/i18n'
import type {
  SessionOptionDescriptor,
  SessionOptionSelectChoice,
  SessionOptionValue
} from '../../../../shared/native-chat-session-options'
import { nativeChatSessionChoiceLabel } from './native-chat-session-option-labels'

export type NativeChatModelPickerRowsProps = {
  descriptor: SessionOptionDescriptor
  pending: boolean
  setValue: (value: SessionOptionValue) => void
}

function ModelChoiceBody(props: {
  choice: SessionOptionSelectChoice
  current?: boolean
}): React.JSX.Element {
  return (
    <div className="min-w-0 py-0.5">
      <div className={props.current ? 'text-[13px] font-semibold' : undefined}>
        {nativeChatSessionChoiceLabel(props.choice)}
      </div>
      {props.choice.description ? (
        <div
          className={
            props.current
              ? 'text-xs leading-4 font-normal text-primary-foreground/75 dark:text-muted-foreground'
              : 'text-xs leading-4 font-normal text-muted-foreground'
          }
        >
          {props.choice.description}
        </div>
      ) : null}
    </div>
  )
}

export function NativeChatModelPickerRows({
  descriptor,
  pending,
  setValue
}: NativeChatModelPickerRowsProps): React.JSX.Element | null {
  if (descriptor.kind.type !== 'select') {
    return null
  }
  const currentValue = descriptor.kind.currentValue
  const currentChoice = descriptor.kind.choices.find((choice) => choice.value === currentValue)
  const alternatives = currentChoice
    ? descriptor.kind.choices.filter((choice) => choice.value !== currentChoice.value)
    : descriptor.kind.choices
  const disabled = !descriptor.settable || pending

  return (
    <>
      {currentChoice ? (
        <DropdownMenuRadioGroup value={currentValue} onValueChange={setValue}>
          <DropdownMenuRadioItem
            value={currentChoice.value}
            disabled={disabled}
            aria-current="true"
            className="items-start bg-primary px-3 py-2.5 pl-3 text-primary-foreground focus:bg-primary/90 data-[state=checked]:bg-primary dark:bg-muted dark:text-popover-foreground dark:focus:bg-muted dark:data-[state=checked]:bg-muted [&>span:first-child]:hidden"
          >
            <span aria-hidden="true" className="mt-1.5 size-2 shrink-0 rounded-full bg-white" />
            <ModelChoiceBody choice={currentChoice} current />
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      ) : null}
      {currentChoice && alternatives.length > 0 ? <DropdownMenuSeparator /> : null}
      {alternatives.length > 0 ? (
        <div className="max-h-64 overflow-y-auto scrollbar-sleek">
          <DropdownMenuLabel className="px-3 py-1.5 uppercase tracking-[0.05em]">
            {translate('components.native-chat.composer.availableChoices', 'Available choices')}
          </DropdownMenuLabel>
          <DropdownMenuRadioGroup value={currentValue} onValueChange={setValue}>
            {alternatives.map((choice) => (
              <DropdownMenuRadioItem
                key={choice.value}
                value={choice.value}
                disabled={disabled}
                className="items-start py-1.5 pl-3 [&>span:first-child]:hidden"
              >
                <ModelChoiceBody choice={choice} />
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </div>
      ) : null}
    </>
  )
}
