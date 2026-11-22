import { ChevronDown } from 'lucide-react'
import { useConfirmationDialog } from '@/components/confirmation-dialog-context'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuArrow,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { translate } from '@/i18n/i18n'
import type {
  SessionOptionDescriptor,
  SessionOptionsSurface,
  SessionOptionValue
} from '../../../../shared/native-chat-session-options'
import { nativeChatSessionChoiceLabel } from './native-chat-session-option-labels'
import { runNativeChatSessionOptionSurfaceCall } from './native-chat-session-option-surface-call'

export function NativeChatRunProfilePickers(props: {
  surface: SessionOptionsSurface
  descriptors: SessionOptionDescriptor[]
  isWorking: boolean
  pendingId: string | null
  setPendingId: (id: string | null) => void
}): React.JSX.Element {
  const confirm = useConfirmationDialog()
  const setOption = async (
    descriptor: SessionOptionDescriptor,
    value: SessionOptionValue
  ): Promise<void> => {
    if (descriptor.category === 'permission' && value === 'bypass') {
      const accepted = await confirm({
        title: translate(
          'components.native-chat.composer.confirmBypassPermissionsTitle',
          'Bypass permissions?'
        ),
        description: translate(
          'components.native-chat.composer.confirmBypassPermissions',
          'This removes the provider sandbox and approval prompts where supported. Use it only in an externally isolated environment.'
        ),
        confirmLabel: translate(
          'components.native-chat.composer.confirmBypassPermissionsAction',
          'Bypass permissions'
        ),
        confirmVariant: 'destructive'
      })
      if (!accepted) {
        return
      }
    }
    await runNativeChatSessionOptionSurfaceCall(descriptor.id, props.setPendingId, () =>
      props.surface.setOption(descriptor.id, value)
    )
  }

  return (
    <>
      {props.descriptors.map((descriptor) => {
        const currentValue =
          descriptor.kind.type === 'select' ? descriptor.kind.currentValue : undefined
        const currentChoice =
          descriptor.kind.type === 'select'
            ? descriptor.kind.choices.find((choice) => choice.value === currentValue)
            : null
        const scopeLabel =
          descriptor.applyScope === 'next-session'
            ? translate(
                'components.native-chat.composer.appliesNextSession',
                'Applies to new sessions'
              )
            : translate(
                'components.native-chat.composer.appliesCurrentSession',
                'Applies to this task'
              )
        const label = currentChoice ? nativeChatSessionChoiceLabel(currentChoice) : descriptor.label
        const disabled = props.isWorking || props.pendingId !== null || !descriptor.settable
        return (
          <DropdownMenu key={descriptor.id} modal={false}>
            <Tooltip>
              <TooltipTrigger asChild>
                <DropdownMenuTrigger asChild disabled={disabled}>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-label={`${descriptor.label} · ${scopeLabel} ${label}`}
                    className="max-w-48 gap-1.5 text-muted-foreground transition-colors hover:text-foreground pointer-coarse:h-11"
                  >
                    {currentChoice ? (
                      <>
                        <span
                          data-native-chat-picker-category
                          className="shrink-0 text-[11px] text-muted-foreground/80"
                        >
                          {descriptor.label}
                        </span>
                        <span className="truncate text-xs font-medium text-foreground">
                          {label}
                        </span>
                      </>
                    ) : (
                      <span className="truncate">{label}</span>
                    )}
                    <ChevronDown className="size-3 shrink-0 opacity-60" />
                  </Button>
                </DropdownMenuTrigger>
              </TooltipTrigger>
              <TooltipContent side="top" sideOffset={4}>
                {descriptor.label} · {scopeLabel}
              </TooltipContent>
            </Tooltip>
            <DropdownMenuContent
              align="start"
              side="top"
              sideOffset={8}
              collisionPadding={8}
              className="w-72 max-w-[calc(100vw-1rem)] overflow-visible rounded-[10px] border-border bg-popover text-popover-foreground shadow-[0_10px_24px_rgb(0_0_0/0.18)] backdrop-blur-none dark:border-border dark:bg-popover dark:shadow-[0_10px_24px_rgb(0_0_0/0.18)]"
            >
              <DropdownMenuLabel className="whitespace-normal font-normal text-muted-foreground">
                {descriptor.description}
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              {descriptor.kind.type === 'select' ? (
                <DropdownMenuRadioGroup
                  value={descriptor.kind.currentValue}
                  onValueChange={(value) => void setOption(descriptor, value)}
                >
                  {descriptor.kind.choices.map((choice) => (
                    <DropdownMenuRadioItem
                      key={choice.value}
                      value={choice.value}
                      disabled={disabled || choice.disabled}
                      title={choice.disabledReason}
                    >
                      <div className="min-w-0 py-0.5">
                        <div>{nativeChatSessionChoiceLabel(choice)}</div>
                        {(choice.disabledReason ?? choice.description) ? (
                          <div className="text-xs font-normal text-muted-foreground">
                            {choice.disabledReason ?? choice.description}
                          </div>
                        ) : null}
                      </div>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              ) : null}
              <DropdownMenuArrow className="native-chat-picker-arrow" width={16} height={8} />
            </DropdownMenuContent>
          </DropdownMenu>
        )
      })}
    </>
  )
}
