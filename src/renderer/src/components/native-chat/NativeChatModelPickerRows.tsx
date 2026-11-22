import { useState } from 'react'
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
import type { ModelCatalogPricing } from '../../../../shared/model-catalog-metadata'
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
  const metadata = props.choice.modelMetadata
  const context = formatTokenCapacity(metadata?.contextWindowTokens)
  const price = formatModelPrice(metadata?.pricing)
  const provider = metadata?.providerLabel
  const available = metadata?.availability !== 'unavailable'
  return (
    <div className="grid w-full min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-x-3 py-0.5">
      <div className="min-w-0">
        <div className={props.current ? 'truncate text-[13px] font-semibold' : 'truncate'}>
          {nativeChatSessionChoiceLabel(props.choice)}
        </div>
        {provider || metadata?.availability === 'unavailable' ? (
          <div
            className={
              props.current
                ? 'mt-0.5 flex items-center gap-1.5 text-[11px] leading-4 text-primary-foreground/75 dark:text-muted-foreground'
                : 'mt-0.5 flex items-center gap-1.5 text-[11px] leading-4 text-muted-foreground'
            }
          >
            <span
              aria-hidden="true"
              className={`size-1.5 shrink-0 rounded-full ${
                available ? 'bg-status-success' : 'bg-muted-foreground/60'
              }`}
            />
            <span className="truncate">
              {provider ??
                translate('components.native-chat.composer.modelUnavailable', 'Unavailable')}
            </span>
            <span className="sr-only">
              {available
                ? translate('components.native-chat.composer.providerConnected', 'Connected')
                : translate(
                    'components.native-chat.composer.providerNotConnected',
                    'Provider not connected'
                  )}
            </span>
          </div>
        ) : null}
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
      {context || price ? (
        <div
          className={`readout flex shrink-0 items-center gap-2 self-center text-[11px] ${
            props.current
              ? 'text-primary-foreground/80 dark:text-muted-foreground'
              : 'text-muted-foreground'
          }`}
          title={modelCapacityAndPriceTitle(props.choice)}
        >
          {context ? <span>{context}</span> : null}
          {price ? (
            <span
              className={
                price.free
                  ? props.current
                    ? 'font-semibold'
                    : 'font-semibold text-status-success'
                  : undefined
              }
            >
              {price.label}
            </span>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

function formatTokenCapacity(tokens: number | undefined): string | null {
  if (tokens === undefined || !Number.isFinite(tokens) || tokens <= 0) {
    return null
  }
  if (tokens >= 1_000_000) {
    const millions = tokens / 1_000_000
    const formatted = Number.isInteger(millions)
      ? String(millions)
      : millions.toFixed(2).replace(/\.?0+$/, '')
    return `${formatted}M`
  }
  return `${Math.round(tokens / 1_000)}K`
}

function formatUsd(value: number): string {
  const maximumFractionDigits = value >= 10 ? 1 : value >= 1 ? 2 : 3
  return `$${value.toFixed(maximumFractionDigits).replace(/\.?0+$/, '')}`
}

function formatModelPrice(
  pricing: ModelCatalogPricing | undefined
): { label: string; free: boolean } | null {
  if (!pricing) {
    return null
  }
  const input = pricing.inputPerMillion
  const output = pricing.outputPerMillion
  if (input === 0 && output === 0) {
    return {
      label: translate('components.native-chat.composer.freeModel', 'Free'),
      free: true
    }
  }
  if (input === undefined && output === undefined) {
    return null
  }
  return {
    label: `${input === undefined ? '—' : formatUsd(input)}/${
      output === undefined ? '—' : formatUsd(output)
    }`,
    free: false
  }
}

function modelCapacityAndPriceTitle(choice: SessionOptionSelectChoice): string | undefined {
  const metadata = choice.modelMetadata
  if (!metadata) {
    return undefined
  }
  const details: string[] = []
  if (metadata.contextWindowTokens) {
    details.push(
      translate(
        'components.native-chat.composer.contextWindowTokens',
        '{{value0}} token context window',
        { value0: metadata.contextWindowTokens.toLocaleString() }
      )
    )
  }
  const price = formatModelPrice(metadata.pricing)
  if (price) {
    details.push(
      price.free
        ? translate('components.native-chat.composer.freeModel', 'Free')
        : translate(
            'components.native-chat.composer.modelPricePerMillion',
            '{{value0}} per 1M input/output tokens',
            { value0: price.label }
          )
    )
  }
  return details.join(' · ') || undefined
}

function modelIsFree(choice: SessionOptionSelectChoice): boolean {
  const pricing = choice.modelMetadata?.pricing
  return pricing?.inputPerMillion === 0 && pricing.outputPerMillion === 0
}

function ModelCatalogControls(props: {
  query: string
  freeOnly: boolean
  connectedCount: number
  setQuery: (query: string) => void
  setFreeOnly: (freeOnly: boolean) => void
}): React.JSX.Element {
  const stopMenuTypeahead = (event: React.SyntheticEvent): void => event.stopPropagation()
  return (
    <div className="border-b border-border/60 px-2.5 pt-2 pb-2">
      <div className="mb-1 flex items-center justify-between gap-3 px-0.5">
        <div className="flex min-w-0 items-center gap-1.5 whitespace-nowrap text-[11px] font-medium text-foreground">
          <span className="size-1.5 shrink-0 rounded-full bg-status-success" aria-hidden="true" />
          <span>
            {translate('components.native-chat.composer.connectedModels', 'Connected models')}
          </span>
          <span className="readout text-muted-foreground">{props.connectedCount}</span>
        </div>
        <span className="readout text-[10px] text-muted-foreground">
          {translate('components.native-chat.composer.modelCatalogLegend', 'CTX · $/1M IN/OUT')}
        </span>
      </div>
      <p className="mb-2 px-0.5 text-[10px] leading-4 text-muted-foreground">
        {translate(
          'components.native-chat.composer.connectedProvidersOnly',
          'Only models from connected providers are shown.'
        )}
      </p>
      <div className="flex items-center gap-1.5">
        <input
          type="search"
          value={props.query}
          aria-label={translate(
            'components.native-chat.composer.searchModels',
            'Search models or providers'
          )}
          placeholder={translate(
            'components.native-chat.composer.searchModels',
            'Search models or providers'
          )}
          className="h-8 min-w-0 flex-1 rounded-md border border-input bg-background px-2.5 text-xs text-foreground outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30"
          onChange={(event) => props.setQuery(event.currentTarget.value)}
          onKeyDown={stopMenuTypeahead}
          onClick={stopMenuTypeahead}
          onPointerDown={stopMenuTypeahead}
        />
        <button
          type="button"
          aria-pressed={props.freeOnly}
          className={`h-8 shrink-0 rounded-md border px-2.5 text-[11px] font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/40 ${
            props.freeOnly
              ? 'border-status-success-border bg-status-success-background text-status-success'
              : 'border-input bg-background text-muted-foreground hover:bg-accent hover:text-foreground'
          }`}
          onPointerDown={stopMenuTypeahead}
          onClick={(event) => {
            event.stopPropagation()
            props.setFreeOnly(!props.freeOnly)
          }}
        >
          {translate('components.native-chat.composer.freeOnly', 'Free only')}
        </button>
      </div>
    </div>
  )
}

export function NativeChatModelPickerRows({
  descriptor,
  pending,
  setValue
}: NativeChatModelPickerRowsProps): React.JSX.Element | null {
  const [query, setQuery] = useState('')
  const [freeOnly, setFreeOnly] = useState(false)
  if (descriptor.kind.type !== 'select') {
    return null
  }
  const currentValue = descriptor.kind.currentValue
  const currentChoice = descriptor.kind.choices.find((choice) => choice.value === currentValue)
  const alternatives = currentChoice
    ? descriptor.kind.choices.filter((choice) => choice.value !== currentChoice.value)
    : descriptor.kind.choices
  const hasCatalogMetadata = descriptor.kind.choices.some((choice) => choice.modelMetadata)
  const normalizedQuery = query.trim().toLocaleLowerCase()
  const filteredAlternatives = alternatives.filter((choice) => {
    if (freeOnly && !modelIsFree(choice)) {
      return false
    }
    if (!normalizedQuery) {
      return true
    }
    const metadata = choice.modelMetadata
    return [choice.label, choice.value, metadata?.providerId, metadata?.providerLabel]
      .filter(Boolean)
      .some((value) => value!.toLocaleLowerCase().includes(normalizedQuery))
  })
  const connectedCount = descriptor.kind.choices.filter(
    (choice) => choice.modelMetadata?.availability === 'available'
  ).length
  const disabled = !descriptor.settable || pending

  return (
    <>
      {currentChoice ? (
        <DropdownMenuRadioGroup value={currentValue} onValueChange={setValue}>
          <DropdownMenuRadioItem
            value={currentChoice.value}
            disabled={disabled || currentChoice.disabled}
            title={currentChoice.disabledReason}
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
        <>
          {hasCatalogMetadata ? (
            <ModelCatalogControls
              query={query}
              freeOnly={freeOnly}
              connectedCount={connectedCount}
              setQuery={setQuery}
              setFreeOnly={setFreeOnly}
            />
          ) : null}
          <div className="max-h-72 overflow-y-auto scrollbar-sleek">
            {!hasCatalogMetadata ? (
              <DropdownMenuLabel className="px-3 py-1.5 uppercase tracking-[0.05em]">
                {translate('components.native-chat.composer.availableChoices', 'Available choices')}
              </DropdownMenuLabel>
            ) : null}
            {filteredAlternatives.length > 0 ? (
              <DropdownMenuRadioGroup value={currentValue} onValueChange={setValue}>
                {filteredAlternatives.map((choice) => (
                  <DropdownMenuRadioItem
                    key={choice.value}
                    value={choice.value}
                    disabled={disabled || choice.disabled}
                    title={choice.disabledReason}
                    className="items-start py-1.5 pl-3 [&>span:first-child]:hidden"
                  >
                    <ModelChoiceBody choice={choice} />
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            ) : (
              <div className="px-3 py-6 text-center text-xs text-muted-foreground">
                {translate(
                  'components.native-chat.composer.noMatchingModels',
                  'No matching models'
                )}
              </div>
            )}
          </div>
        </>
      ) : null}
    </>
  )
}
