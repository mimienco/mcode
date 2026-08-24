import { memo, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuArrow,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { translate } from '@/i18n/i18n'
import { sortNativeChatSessionOptions } from '../../../../shared/native-chat-session-option-snapshot'
import type {
  SessionOptionDescriptor,
  SessionOptionsSurface,
  SessionOptionValue
} from '../../../../shared/native-chat-session-options'
import {
  nativeChatModelPillLabel,
  nativeChatOptionsPillLabel,
  nativeChatOptionsPillTitle,
  nativeChatSessionChoiceLabel,
  nativeChatSessionOptionDisabledReason,
  nativeChatSessionOptionLabel
} from './native-chat-session-option-labels'
import { NativeChatEffortSlider } from './NativeChatEffortSlider'
import { NativeChatModelPickerRows } from './NativeChatModelPickerRows'
import { NativeChatRunProfilePickers } from './NativeChatRunProfilePickers'
import { runNativeChatSessionOptionSurfaceCall } from './native-chat-session-option-surface-call'

export type NativeChatSessionOptionPickersProps = {
  surface: SessionOptionsSurface | null
  snapshot: SessionOptionDescriptor[]
  isWorking: boolean
}

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

function PickerTrigger(props: {
  label: string
  tooltipLabel: string
  disabled: boolean
  disabledReason?: string | null
  dispatched: boolean
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
              className="max-w-48 text-muted-foreground"
            >
              <span className="truncate">{props.label}</span>
              <ChevronDown className="size-3" />
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

function ChoiceBody(props: { label: string; description?: string }): React.JSX.Element {
  return (
    <div className="min-w-0 py-0.5">
      <div>{props.label}</div>
      {props.description ? (
        <div className="text-xs font-normal text-muted-foreground">{props.description}</div>
      ) : null}
    </div>
  )
}

function DescriptorMenuRows(props: {
  descriptor: SessionOptionDescriptor
  pending: boolean
  setValue: (value: SessionOptionValue) => void
  invokeAction: () => void
}): React.JSX.Element {
  const { descriptor, pending, setValue, invokeAction } = props
  // Why: flip-only without a baseline is an action — never claim On/Off.
  if (descriptor.action?.type === 'toggle-command') {
    return (
      <DropdownMenuItem disabled={!descriptor.settable || pending} onSelect={() => invokeAction()}>
        {translate('components.native-chat.composer.toggleOption', 'Toggle {{value0}}', {
          value0: nativeChatSessionOptionLabel(descriptor).toLowerCase()
        })}
      </DropdownMenuItem>
    )
  }
  // Why: agent-picker opens the TUI; it is not a set of radio choices.
  if (descriptor.action?.type === 'agent-picker') {
    if (descriptor.category === 'model') {
      return (
        <>
          <DropdownMenuItem
            disabled={!descriptor.settable || pending}
            onSelect={() => invokeAction()}
          >
            {translate(
              'components.native-chat.composer.chooseModelInTerminal',
              'Choose model in Terminal…'
            )}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="max-w-64 whitespace-normal font-normal text-muted-foreground">
            {translate(
              'components.native-chat.composer.directModelSwitchUnavailable',
              'Direct model switching is unavailable for this session. Choose in Terminal or start a new Codex session.'
            )}
          </DropdownMenuLabel>
        </>
      )
    }
    const choices = descriptor.kind.type === 'select' ? descriptor.kind.choices : []
    const choicesLabel = translate(
      'components.native-chat.composer.availableChoices',
      'Available choices'
    )
    return (
      <>
        <DropdownMenuItem
          disabled={!descriptor.settable || pending}
          onSelect={() => invokeAction()}
        >
          {translate(
            'components.native-chat.composer.chooseInAgentPicker',
            'Choose in agent picker…'
          )}
        </DropdownMenuItem>
        {choices.length > 0 ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="font-normal text-muted-foreground">
              {choicesLabel}
            </DropdownMenuLabel>
            <div role="list" aria-label={choicesLabel}>
              {choices.map((choice) => {
                const label = nativeChatSessionChoiceLabel(choice)
                return (
                  <div
                    key={choice.value}
                    role="listitem"
                    aria-label={label}
                    className="px-2 py-1 text-xs leading-5 font-medium text-muted-foreground"
                  >
                    <ChoiceBody label={label} description={choice.description} />
                  </div>
                )
              })}
            </div>
          </>
        ) : null}
      </>
    )
  }
  // Why: absolute On/Off only when we have tracked truth. Unknown composed
  // booleans leave the group unselected so empty radios are not a selection.
  if (descriptor.kind.type === 'boolean') {
    const selected =
      descriptor.kind.currentValue === true
        ? 'on'
        : descriptor.kind.currentValue === false
          ? 'off'
          : undefined
    return (
      <>
        {selected === undefined ? (
          <DropdownMenuLabel className="font-normal text-muted-foreground">
            {translate(
              'components.native-chat.composer.valueUnknown',
              'Current value unknown — pick On or Off'
            )}
          </DropdownMenuLabel>
        ) : null}
        <DropdownMenuRadioGroup value={selected} onValueChange={(next) => setValue(next === 'on')}>
          <DropdownMenuRadioItem value="on" disabled={!descriptor.settable || pending}>
            {translate('components.native-chat.composer.optionValue.on', 'On')}
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="off" disabled={!descriptor.settable || pending}>
            {translate('components.native-chat.composer.optionValue.off', 'Off')}
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </>
    )
  }
  if (descriptor.category === 'model' && descriptor.kind.type === 'select') {
    return (
      <NativeChatModelPickerRows descriptor={descriptor} pending={pending} setValue={setValue} />
    )
  }
  return (
    <DropdownMenuRadioGroup
      value={descriptor.kind.currentValue}
      onValueChange={(value) => setValue(value)}
    >
      {descriptor.kind.choices.map((choice) => (
        <DropdownMenuRadioItem
          key={choice.value}
          value={choice.value}
          disabled={!descriptor.settable || pending || choice.disabled}
          title={choice.disabledReason}
        >
          <ChoiceBody
            label={nativeChatSessionChoiceLabel(choice)}
            description={choice.disabledReason ?? choice.description}
          />
        </DropdownMenuRadioItem>
      ))}
    </DropdownMenuRadioGroup>
  )
}

function NativeChatSessionOptionPickersInner({
  surface,
  snapshot,
  isWorking
}: NativeChatSessionOptionPickersProps): React.JSX.Element | null {
  const [pendingId, setPendingId] = useState<string | null>(null)
  const model = snapshot.find((descriptor) => descriptor.category === 'model')
  const effortDescriptor = snapshot.find(
    (descriptor): descriptor is SessionOptionDescriptor & { kind: { type: 'select' } } =>
      descriptor.id === 'effort' && descriptor.kind.type === 'select'
  )
  const runProfileDescriptors = snapshot.filter(
    (descriptor) =>
      descriptor.category === 'permission' || descriptor.category === 'interaction_mode'
  )
  const options = sortNativeChatSessionOptions(
    snapshot.filter(
      (descriptor) =>
        descriptor !== effortDescriptor &&
        descriptor !== model &&
        !runProfileDescriptors.includes(descriptor)
    )
  )
  if (!surface || (!model && runProfileDescriptors.length === 0)) {
    return null
  }

  const setOption = (
    descriptor: SessionOptionDescriptor,
    value: SessionOptionValue
  ): Promise<boolean> => {
    return runNativeChatSessionOptionSurfaceCall(descriptor.id, setPendingId, () =>
      surface.setOption(descriptor.id, value)
    )
  }
  const invokeAction = (descriptor: SessionOptionDescriptor): void => {
    void runNativeChatSessionOptionSurfaceCall(descriptor.id, setPendingId, () =>
      surface.invokeAction(descriptor.id)
    )
  }

  const modelReason = model
    ? nativeChatSessionOptionDisabledReason(model.disabledReason)
    : null
  const modelTooltip = translate('components.native-chat.composer.model', 'Model')
  const optionsTooltip = nativeChatOptionsPillTitle(options)
  const allOptionsDisabled = options.every((descriptor) => !descriptor.settable)
  const optionsReason =
    options.length > 0 && allOptionsDisabled
      ? nativeChatSessionOptionDisabledReason(options[0]?.disabledReason)
      : null

  return (
    <div className="flex min-w-0 items-center gap-0.5">
      <NativeChatRunProfilePickers
        surface={surface}
        descriptors={runProfileDescriptors}
        isWorking={isWorking}
        pendingId={pendingId}
        setPendingId={setPendingId}
      />
      {effortDescriptor ? (
        <NativeChatEffortSlider
          descriptor={effortDescriptor}
          disabled={isWorking || pendingId !== null}
          saving={pendingId === effortDescriptor.id}
          onValueChange={(value) => setOption(effortDescriptor, value)}
        />
      ) : null}
      {options.length > 0 ? (
        <>
          <DropdownMenu modal={false}>
            <PickerTrigger
              label={nativeChatOptionsPillLabel(options)}
              tooltipLabel={optionsTooltip}
              disabled={isWorking || pendingId !== null || allOptionsDisabled}
              disabledReason={optionsReason}
              dispatched={options.some((descriptor) => descriptor.valueSource === 'dispatched')}
            />
            <DropdownMenuContent
              align="start"
              side="top"
              sideOffset={8}
              collisionPadding={8}
              className="w-60"
            >
              {options.map((descriptor, index) => {
                const reason = nativeChatSessionOptionDisabledReason(descriptor.disabledReason)
                return (
                  <div key={descriptor.id}>
                    {index > 0 ? <DropdownMenuSeparator /> : null}
                    <DropdownMenuLabel>
                      {nativeChatSessionOptionLabel(descriptor)}
                    </DropdownMenuLabel>
                    {reason && !descriptor.settable ? (
                      <DropdownMenuLabel className="font-normal">{reason}</DropdownMenuLabel>
                    ) : null}
                    <DescriptorMenuRows
                      descriptor={descriptor}
                      pending={pendingId !== null}
                      setValue={(value) => setOption(descriptor, value)}
                      invokeAction={() => invokeAction(descriptor)}
                    />
                  </div>
                )
              })}
              <DropdownMenuArrow className="native-chat-picker-arrow" width={16} height={8} />
            </DropdownMenuContent>
          </DropdownMenu>
          {optionsReason ? (
            <span role="status" className="px-1 text-xs text-muted-foreground">
              {optionsReason}
            </span>
          ) : null}
        </>
      ) : null}
      {model ? (
        <DropdownMenu modal={false}>
          <PickerTrigger
            label={nativeChatModelPillLabel(model)}
            tooltipLabel={modelTooltip}
            disabled={isWorking || pendingId !== null}
            disabledReason={modelReason}
            dispatched={model.valueSource === 'dispatched'}
          />
          <DropdownMenuContent
            align="start"
            side="top"
            sideOffset={8}
            collisionPadding={8}
            className="w-80 max-w-[calc(100vw-1rem)] overflow-visible rounded-[10px] border-border bg-popover text-popover-foreground shadow-[0_10px_24px_rgb(0_0_0/0.18)] backdrop-blur-none dark:border-border dark:bg-popover dark:shadow-[0_10px_24px_rgb(0_0_0/0.18)]"
          >
          {modelReason && !model.settable ? (
            <DropdownMenuLabel className="font-normal">{modelReason}</DropdownMenuLabel>
          ) : null}
          <DescriptorMenuRows
            descriptor={model}
            pending={pendingId !== null}
            setValue={(value) => setOption(model, value)}
            invokeAction={() => invokeAction(model)}
          />
          <DropdownMenuArrow className="native-chat-picker-arrow" width={16} height={8} />
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </div>
  )
}

export const NativeChatSessionOptionPickers = memo(NativeChatSessionOptionPickersInner)
