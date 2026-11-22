import { useCallback, useEffect, useId, useRef, useState, type CSSProperties } from 'react'
import { BrainCircuit, ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverArrow, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { translate } from '@/i18n/i18n'
import type { SessionOptionDescriptor } from '../../../../shared/native-chat-session-options'
import {
  nativeChatSessionChoiceLabel,
  nativeChatSessionOptionDisabledReason
} from './native-chat-session-option-labels'

export type NativeChatEffortSliderProps = {
  descriptor: SessionOptionDescriptor & { kind: { type: 'select' } }
  disabled: boolean
  saving: boolean
  onValueChange: (value: string) => Promise<boolean>
}

type EffortMotionStyle = CSSProperties & Record<`--${string}`, string>

const EFFORT_WAVE_WIDTH = 288
const EFFORT_WAVE_HEIGHT = 52
const EFFORT_TRACK_INSET = 5
const EFFORT_CONFIRMATION_TIMEOUT_MS = 10_000

function buildEffortWavePath(): string {
  const samples = 220
  const points: string[] = []
  for (let index = 0; index <= samples; index += 1) {
    const progress = index / samples
    const x = progress * EFFORT_WAVE_WIDTH
    const amplitude = 3 + progress ** 1.55 * 15
    const phase = Math.PI * 2 * (5 * progress + 10 * progress ** 2)
    const y = EFFORT_WAVE_HEIGHT / 2 + Math.sin(phase) * amplitude
    points.push(`${index === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`)
  }
  return points.join(' ')
}

const EFFORT_WAVE_PATH = buildEffortWavePath()

function effortPosition(index: number, maxIndex: number): number {
  if (maxIndex === 0) {
    return 50
  }
  return EFFORT_TRACK_INSET + (index / maxIndex) * (100 - EFFORT_TRACK_INSET * 2)
}

function effortWavePosition(index: number, maxIndex: number): number {
  if (maxIndex === 0) {
    return 100
  }
  const progress = Math.min(1, Math.max(0, index / maxIndex))
  return EFFORT_TRACK_INSET + progress * (100 - EFFORT_TRACK_INSET)
}

function effortAccent(index: number, maxIndex: number): string {
  const intensity = maxIndex === 0 ? 0 : index / maxIndex
  const heat = Math.round(intensity ** 1.25 * 90)
  return `color-mix(in oklch, var(--ring) ${100 - heat}%, var(--native-chat-effort-heat))`
}

function effortMotionStyle(index: number, maxIndex: number): EffortMotionStyle {
  const intensity = maxIndex === 0 ? 0 : index / maxIndex
  const flowRate = 1 + intensity * 2.65
  return {
    '--effort-cycle-duration': `${(4.2 / flowRate).toFixed(2)}s`,
    '--effort-flow-rate': flowRate.toFixed(2),
    '--effort-signal-opacity': `${(0.68 + intensity * 0.25).toFixed(2)}`
  }
}

type EffortSliderBodyProps = {
  choices: { value: string; label: string }[]
  hasValue: boolean
  currentIndex: number
  interactive: boolean
  saving: boolean
  onCommitIndex: (index: number) => void
}

type DragPosition = {
  index: number
  percent: number
}

/** The draggable power slider inside the popover. Motion cadence communicates
 *  how much reasoning work the selected level asks the agent to do. */
export function NativeChatEffortSliderBody(props: EffortSliderBodyProps): React.JSX.Element {
  const { choices, hasValue, currentIndex, interactive, saving, onCommitIndex } = props
  const [dragPosition, setDragPosition] = useState<DragPosition | null>(null)
  const trackRef = useRef<HTMLDivElement>(null)
  const gradientId = useId()
  const clipId = useId()
  const maxIndex = Math.max(0, choices.length - 1)
  // Why: the surface's value only updates after the agent accepts the switch, so
  // the slider keeps its own committed index — otherwise the thumb jumps back to
  // the old level and animates forward again once the snapshot catches up.
  const [localIndex, setLocalIndex] = useState(currentIndex)
  const [localHasValue, setLocalHasValue] = useState(hasValue)
  useEffect(() => {
    // A session-option dispatch may briefly publish `unknown`. Keep the last
    // visible value until a real snapshot arrives so persistence never blanks
    // the waveform the user just selected.
    if (!hasValue) {
      return
    }
    setLocalIndex(currentIndex)
    setLocalHasValue(true)
  }, [currentIndex, hasValue])
  const activeIndex = dragPosition?.index ?? localIndex
  const percent = dragPosition?.percent ?? effortPosition(activeIndex, maxIndex)
  const wavePercent = choices.length === 0 ? 0 : effortWavePosition(activeIndex, maxIndex)
  const dragging = dragPosition !== null
  const displaysValue = dragging || localHasValue
  const emptyLabel = translate('components.native-chat.effort.chooseLevel', 'Choose level')
  const activeLabel =
    displaysValue && activeIndex < choices.length
      ? nativeChatSessionChoiceLabel(choices[activeIndex])
      : emptyLabel
  const activeColor = effortAccent(activeIndex, maxIndex)
  const pulseLength = Math.min(18, Math.max(4, wavePercent * 0.7))
  const motionStyle: EffortMotionStyle = {
    ...effortMotionStyle(activeIndex, maxIndex),
    '--effort-position': `${percent}%`,
    '--effort-accent': activeColor,
    '--effort-wave-position': `${wavePercent}%`,
    '--effort-path-offset': `${-wavePercent}`,
    '--effort-pulse-length': pulseLength.toFixed(2),
    '--effort-pulse-gap': Math.max(0.1, wavePercent - pulseLength).toFixed(2)
  }
  const commit = useCallback(
    (index: number) => {
      setDragPosition(null)
      if (choices[index]) {
        setLocalIndex(index)
        setLocalHasValue(true)
      }
      onCommitIndex(index)
    },
    [choices, onCommitIndex]
  )

  const positionFromClientX = useCallback(
    (clientX: number): DragPosition | null => {
      const rect = trackRef.current?.getBoundingClientRect()
      if (!rect || rect.width === 0) {
        return null
      }
      const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
      return {
        index: Math.round(ratio * maxIndex),
        percent: EFFORT_TRACK_INSET + ratio * (100 - EFFORT_TRACK_INSET * 2)
      }
    },
    [maxIndex]
  )

  return (
    <div className="native-chat-effort-surface" style={motionStyle}>
      <div className="native-chat-effort-header flex items-center justify-between px-4 py-3">
        <span className="text-sm font-medium text-popover-foreground">
          {translate('components.native-chat.effort.sliderTitle', 'Effort')}
        </span>
        <span
          aria-live="polite"
          className="native-chat-effort-value min-w-20 text-right text-sm font-medium"
        >
          {activeLabel}
        </span>
      </div>
      <div
        ref={trackRef}
        role="slider"
        aria-label={translate('components.native-chat.effort.sliderTitle', 'Effort')}
        aria-valuemin={0}
        aria-valuemax={maxIndex}
        aria-valuenow={displaysValue ? activeIndex : undefined}
        aria-valuetext={activeLabel}
        aria-orientation="horizontal"
        aria-disabled={!interactive}
        tabIndex={interactive ? 0 : -1}
        data-active={displaysValue}
        data-dragging={dragging}
        className={cn(
          'native-chat-effort-slider group relative mx-4 mb-3 h-20 cursor-pointer touch-none select-none rounded-md outline-none',
          'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-popover',
          !interactive && 'pointer-events-none',
          !interactive && !saving && 'opacity-50'
        )}
        onPointerDown={(event) => {
          if (!interactive) {
            return
          }
          event.currentTarget.setPointerCapture(event.pointerId)
          setDragPosition(positionFromClientX(event.clientX) ?? { index: activeIndex, percent })
        }}
        onPointerMove={(event) => {
          if (dragPosition === null) {
            return
          }
          setDragPosition(positionFromClientX(event.clientX) ?? dragPosition)
        }}
        onPointerUp={() => {
          if (dragPosition !== null) {
            commit(dragPosition.index)
          }
        }}
        onPointerCancel={() => setDragPosition(null)}
        onLostPointerCapture={() => setDragPosition(null)}
        onKeyDown={(event) => {
          if (!interactive) {
            return
          }
          if ((event.key === 'ArrowRight' || event.key === 'ArrowUp') && activeIndex < maxIndex) {
            event.preventDefault()
            commit(Math.min(maxIndex, activeIndex + 1))
          } else if ((event.key === 'ArrowLeft' || event.key === 'ArrowDown') && activeIndex > 0) {
            event.preventDefault()
            commit(Math.max(0, activeIndex - 1))
          } else if (event.key === 'Home') {
            event.preventDefault()
            commit(0)
          } else if (event.key === 'End') {
            event.preventDefault()
            commit(maxIndex)
          }
        }}
      >
        <svg
          aria-hidden="true"
          className="native-chat-effort-wave"
          viewBox={`0 0 ${EFFORT_WAVE_WIDTH} ${EFFORT_WAVE_HEIGHT}`}
          preserveAspectRatio="none"
        >
          <defs>
            <linearGradient id={gradientId} x1="0" x2="1" y1="0" y2="0">
              <stop offset="0%" stopColor="var(--ring)" />
              <stop offset="48%" stopColor="var(--native-chat-effort-warm)" />
              <stop offset="100%" stopColor="var(--native-chat-effort-heat)" />
            </linearGradient>
            <clipPath id={clipId}>
              <rect
                className="native-chat-effort-wave-clip"
                width={EFFORT_WAVE_WIDTH}
                height={EFFORT_WAVE_HEIGHT}
                style={{ transform: `scaleX(${wavePercent / 100})` }}
              />
            </clipPath>
          </defs>
          <path className="native-chat-effort-wave-muted" d={EFFORT_WAVE_PATH} />
          <g clipPath={`url(#${clipId})`}>
            <path
              className="native-chat-effort-wave-active"
              d={EFFORT_WAVE_PATH}
              stroke={`url(#${gradientId})`}
            />
            <path className="native-chat-effort-wave-pulse" d={EFFORT_WAVE_PATH} pathLength="100" />
          </g>
        </svg>
        <div aria-hidden="true" className="native-chat-effort-rail" />
        <div aria-hidden="true" className="native-chat-effort-fill" />
        {choices.map((choice, index) => (
          <div
            key={choice.value}
            aria-hidden="true"
            data-reached={index <= activeIndex}
            data-selected={index === activeIndex}
            className="native-chat-effort-stop"
            style={{
              left: `${effortPosition(index, maxIndex)}%`
            }}
          />
        ))}
        <div aria-hidden="true" className="native-chat-effort-thumb" />
      </div>
    </div>
  )
}

/** Composer pill that pops the power slider in a small card above the bar. */
export function NativeChatEffortSlider({
  descriptor,
  disabled,
  saving,
  onValueChange
}: NativeChatEffortSliderProps): React.JSX.Element {
  const choices = descriptor.kind.choices
  const maxIndex = Math.max(0, choices.length - 1)
  const authoritativeValue = descriptor.kind.currentValue
  const [optimisticValue, setOptimisticValue] = useState<string | null>(null)
  useEffect(() => {
    if (optimisticValue === null) {
      return
    }
    if (authoritativeValue === optimisticValue) {
      setOptimisticValue(null)
      return
    }
    const timeoutId = window.setTimeout(() => {
      setOptimisticValue((value) => (value === optimisticValue ? null : value))
    }, EFFORT_CONFIRMATION_TIMEOUT_MS)
    return () => window.clearTimeout(timeoutId)
  }, [authoritativeValue, optimisticValue])
  const displayedValue = optimisticValue ?? authoritativeValue
  const currentIndex = choices.findIndex((choice) => choice.value === displayedValue)
  const hasValue = currentIndex !== -1
  const interactive = descriptor.settable && !disabled
  const reason = nativeChatSessionOptionDisabledReason(descriptor.disabledReason)
  const currentLabel =
    hasValue && currentIndex < choices.length
      ? nativeChatSessionChoiceLabel(choices[currentIndex])
      : translate('components.native-chat.composer.valueUnknownShort', '?')
  const triggerLabel = hasValue
    ? currentLabel
    : translate('components.native-chat.effort.sliderTitle', 'Effort')
  const accessibleLabel = hasValue
    ? translate('components.native-chat.effort.sliderLabelWithValue', 'Effort: {{value}}', {
        value: currentLabel
      })
    : translate('components.native-chat.effort.sliderLabel', 'Effort')

  const commitIndex = useCallback(
    (index: number) => {
      const choice = choices[index]
      if (choice && index !== currentIndex) {
        setOptimisticValue(choice.value)
        void onValueChange(choice.value).then(
          (succeeded) => {
            if (!succeeded) {
              setOptimisticValue((value) => (value === choice.value ? null : value))
            }
          },
          () => {
            setOptimisticValue((value) => (value === choice.value ? null : value))
          }
        )
      }
    },
    [choices, currentIndex, onValueChange]
  )

  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={disabled}
              aria-label={accessibleLabel}
              aria-busy={saving}
              className="native-chat-effort-trigger w-32 justify-start gap-1.5 text-muted-foreground transition-colors hover:text-foreground pointer-coarse:h-11"
            >
              <BrainCircuit
                className="size-3.5 shrink-0 transition-colors duration-300"
                style={{ color: hasValue ? effortAccent(currentIndex, maxIndex) : undefined }}
              />
              {hasValue ? (
                <>
                  <span
                    data-native-chat-picker-category
                    className="shrink-0 text-[11px] text-muted-foreground/80"
                  >
                    {translate('components.native-chat.effort.sliderTitle', 'Effort')}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-left text-xs font-medium text-foreground">
                    {currentLabel}
                  </span>
                </>
              ) : (
                <span className="min-w-0 flex-1 truncate text-left">{triggerLabel}</span>
              )}
              <ChevronDown className="size-3 shrink-0 opacity-60" />
            </Button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top" sideOffset={4}>
          {reason ?? accessibleLabel}
        </TooltipContent>
      </Tooltip>
      <PopoverContent
        side="top"
        align="start"
        sideOffset={8}
        collisionPadding={8}
        className="native-chat-effort-popover w-[min(20rem,calc(100vw-1rem))] overflow-visible rounded-[10px] border-border bg-popover p-0 text-popover-foreground shadow-[0_10px_24px_rgb(0_0_0/0.18)] backdrop-blur-none dark:border-border dark:bg-popover dark:shadow-[0_10px_24px_rgb(0_0_0/0.18)]"
      >
        <NativeChatEffortSliderBody
          choices={choices.map((choice) => ({
            value: choice.value,
            label: nativeChatSessionChoiceLabel(choice)
          }))}
          hasValue={hasValue}
          currentIndex={Math.max(0, currentIndex)}
          interactive={interactive}
          saving={saving}
          onCommitIndex={commitIndex}
        />
        <PopoverArrow className="native-chat-picker-arrow" width={16} height={8} />
      </PopoverContent>
    </Popover>
  )
}
