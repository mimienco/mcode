import { memo, useState } from 'react'
import { SlidersHorizontal } from 'lucide-react'
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
import { PickerTrigger } from './native-chat-picker-trigger'
import { NativeChatModelPickerRows } from './NativeChatModelPickerRows'
import { NativeChatRunProfilePickers } from './NativeChatRunProfilePickers'
import { runNativeChatSessionOptionSurfaceCall } from './native-chat-session-option-surface-call'

export type NativeChatSessionOptionPickersProps = {
  surface: SessionOptionsSurface | null
  snapshot: SessionOptionDescriptor[]
  isWorking: boolean
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
  const [compactOpen, setCompactOpen] = useState(false)
  const [compactDescriptorId, setCompactDescriptorId] = useState<string | null>(null)
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
  const compactDescriptors = [...sortNativeChatSessionOptions(snapshot), ...(model ? [model] : [])]
  const compactDescriptor =
    compactDescriptors.find((descriptor) => descriptor.id === compactDescriptorId) ??
    compactDescriptors[0]
  const compactAllOptionsDisabled = compactDescriptors.every((descriptor) => !descriptor.settable)
  const compactLabel = translate(
    'components.native-chat.composer.sessionSettings',
    'Session settings'
  )
  const compactFiltersLabel = translate(
    'components.native-chat.composer.sessionSettingsFilters',
    'Session setting categories'
  )
  if (!surface || compactDescriptors.length === 0) {
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

  const modelReason = model ? nativeChatSessionOptionDisabledReason(model.disabledReason) : null
  const modelTooltip = translate('components.native-chat.composer.model', 'Model')
  const optionsTooltip = nativeChatOptionsPillTitle(options)
  const allOptionsDisabled = options.every((descriptor) => !descriptor.settable)
  const optionsReason =
    options.length > 0 && allOptionsDisabled
      ? nativeChatSessionOptionDisabledReason(options[0]?.disabledReason)
      : null

  return (
    <>
      <div
        data-native-chat-session-options
        className="flex min-w-0 max-w-full flex-nowrap items-center justify-end gap-0.5"
      >
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
              valueClassName="readout"
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
      <div data-native-chat-compact-options className="hidden items-center">
        <DropdownMenu modal={false} open={compactOpen} onOpenChange={setCompactOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={compactLabel}
              title={compactLabel}
              disabled={isWorking || pendingId !== null || compactAllOptionsDisabled}
              onClick={() => setCompactOpen(true)}
            >
              <SlidersHorizontal className="size-4" />
            </Button>
          </DropdownMenuTrigger>
          {compactOpen && compactDescriptor ? (
            <DropdownMenuContent
              align="start"
              side="top"
              sideOffset={8}
              collisionPadding={8}
              className="w-72 max-w-[calc(100vw-1rem)] overflow-hidden p-0"
            >
              <div
                data-native-chat-compact-option-panel
                className="scrollbar-sleek max-h-[min(22rem,calc(100vh-7rem))] overflow-y-auto p-1"
              >
                <DropdownMenuLabel>
                  {nativeChatSessionOptionLabel(compactDescriptor)}
                </DropdownMenuLabel>
                {compactDescriptor.disabledReason && !compactDescriptor.settable ? (
                  <DropdownMenuLabel className="font-normal">
                    {nativeChatSessionOptionDisabledReason(compactDescriptor.disabledReason)}
                  </DropdownMenuLabel>
                ) : null}
                <DescriptorMenuRows
                  descriptor={compactDescriptor}
                  pending={pendingId !== null}
                  setValue={(value) => setOption(compactDescriptor, value)}
                  invokeAction={() => invokeAction(compactDescriptor)}
                />
              </div>
              <div
                data-native-chat-compact-option-filters
                role="tablist"
                aria-label={compactFiltersLabel}
                className="grid gap-0.5 border-t border-border/60 bg-popover p-1"
                style={{
                  gridTemplateColumns: `repeat(${compactDescriptors.length}, minmax(0, 1fr))`
                }}
              >
                {compactDescriptors.map((descriptor) => {
                  const selected = descriptor.id === compactDescriptor.id
                  const filterLabel =
                    descriptor.id === 'effort'
                      ? translate('components.native-chat.composer.effort', 'Effort')
                      : descriptor.label
                  return (
                    <Button
                      key={descriptor.id}
                      type="button"
                      role="tab"
                      aria-selected={selected}
                      aria-label={filterLabel}
                      title={descriptor.label}
                      variant="ghost"
                      size="xs"
                      className={`h-8 min-w-0 truncate rounded-md px-1.5 text-[10px] ${
                        selected
                          ? 'bg-accent text-foreground'
                          : 'text-muted-foreground hover:text-foreground'
                      }`}
                      onClick={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        setCompactDescriptorId(descriptor.id)
                      }}
                    >
                      {filterLabel}
                    </Button>
                  )
                })}
              </div>
              <DropdownMenuArrow className="native-chat-picker-arrow" width={16} height={8} />
            </DropdownMenuContent>
          ) : null}
        </DropdownMenu>
      </div>
    </>
  )
}

export const NativeChatSessionOptionPickers = memo(NativeChatSessionOptionPickersInner)
