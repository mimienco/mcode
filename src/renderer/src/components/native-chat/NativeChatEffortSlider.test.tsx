// @vitest-environment happy-dom

import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { i18n } from '../../i18n/i18n'
import { TooltipProvider } from '../ui/tooltip'
import { NativeChatEffortSlider, NativeChatEffortSliderBody } from './NativeChatEffortSlider'

const CHOICES = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' }
]

function renderBody(props: {
  hasValue?: boolean
  currentIndex?: number
  interactive?: boolean
  saving?: boolean
  onCommitIndex?: (index: number) => void
}): { root: Root; container: HTMLElement } {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => {
    root.render(
      <TooltipProvider>
        <NativeChatEffortSliderBody
          choices={CHOICES}
          hasValue={props.hasValue ?? true}
          currentIndex={props.currentIndex ?? 1}
          interactive={props.interactive ?? true}
          saving={props.saving ?? false}
          onCommitIndex={props.onCommitIndex ?? vi.fn()}
        />
      </TooltipProvider>
    )
  })
  return { root, container }
}

describe('NativeChatEffortSliderBody', () => {
  it('reserves one stable trigger width for every effort label', () => {
    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    const renderValue = (currentValue: string | undefined): void => {
      act(() => {
        root.render(
          <TooltipProvider>
            <NativeChatEffortSlider
              descriptor={{
                id: 'effort',
                label: 'Effort',
                category: 'thought_level',
                kind: { type: 'select', currentValue, choices: CHOICES },
                valueSource: currentValue ? 'applied' : 'unknown',
                settable: true
              }}
              disabled={false}
              saving={false}
              onValueChange={vi.fn().mockResolvedValue(true)}
            />
          </TooltipProvider>
        )
      })
    }

    for (const value of [undefined, 'low', 'medium', 'high']) {
      renderValue(value)
      expect(container.querySelector('button')?.className).toContain('w-28')
    }
    root.unmount()
  })

  it('announces the selected effort value and saving state from the trigger', async () => {
    await i18n.changeLanguage('en')
    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    act(() => {
      root.render(
        <TooltipProvider>
          <NativeChatEffortSlider
            descriptor={{
              id: 'effort',
              label: 'Effort',
              category: 'thought_level',
              kind: { type: 'select', currentValue: 'medium', choices: CHOICES },
              valueSource: 'applied',
              settable: true
            }}
            disabled={true}
            saving={true}
            onValueChange={vi.fn().mockResolvedValue(true)}
          />
        </TooltipProvider>
      )
    })

    const trigger = container.querySelector('button')
    expect(trigger?.getAttribute('aria-label')).toBe('Effort: Medium')
    expect(trigger?.getAttribute('aria-busy')).toBe('true')
    act(() => {
      trigger?.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })
    expect(document.body.querySelector('[role="tooltip"]')?.textContent).toContain('Effort: Medium')
    root.unmount()
  })

  it('keeps a responsive, collision-aware popup anchored to the trigger', async () => {
    await i18n.changeLanguage('en')
    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    act(() => {
      root.render(
        <TooltipProvider>
          <NativeChatEffortSlider
            descriptor={{
              id: 'effort',
              label: 'Effort',
              category: 'thought_level',
              kind: { type: 'select', currentValue: 'medium', choices: CHOICES },
              valueSource: 'applied',
              settable: true
            }}
            disabled={false}
            saving={false}
            onValueChange={vi.fn().mockResolvedValue(true)}
          />
        </TooltipProvider>
      )
    })
    act(() => {
      container.querySelector('button')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })

    const popup = document.body.querySelector('.native-chat-effort-popover')
    expect(popup?.className).toContain('w-[min(20rem,calc(100vw-1rem))]')
    expect(popup?.querySelector('[data-slot="popover-arrow"]')?.classList).toContain(
      'native-chat-picker-arrow'
    )
    root.unmount()
  })

  it('renders one live value readout without duplicate stop labels', async () => {
    await i18n.changeLanguage('en')
    const { container, root } = renderBody({ currentIndex: 1 })
    const slider = container.querySelector('[role="slider"]')
    expect(slider?.getAttribute('aria-valuenow')).toBe('1')
    expect(slider?.getAttribute('aria-valuetext')).toBe('Medium')
    expect(container.querySelector('[aria-live="polite"]')?.textContent).toBe('Medium')
    expect(container.querySelectorAll('button')).toHaveLength(0)
    expect(container.querySelector('.native-chat-effort-wave-active')).toBeTruthy()
    expect(container.querySelector('.native-chat-effort-wave-pulse')).toBeTruthy()
    expect(container.querySelector('.native-chat-effort-runner')).toBeNull()
    root.unmount()
  })

  it('increases animation speed and heat with effort', () => {
    const low = renderBody({ currentIndex: 0 })
    const high = renderBody({ currentIndex: 2 })
    const lowStyle = (low.container.firstElementChild as HTMLElement).style
    const highStyle = (high.container.firstElementChild as HTMLElement).style

    expect(Number(highStyle.getPropertyValue('--effort-flow-rate'))).toBeGreaterThan(
      Number(lowStyle.getPropertyValue('--effort-flow-rate'))
    )
    expect(highStyle.getPropertyValue('--effort-accent')).toContain(
      'var(--native-chat-effort-heat)'
    )
    expect(highStyle.getPropertyValue('--effort-accent')).not.toBe(
      lowStyle.getPropertyValue('--effort-accent')
    )

    low.root.unmount()
    high.root.unmount()
  })

  it('bounds the low-effort waveform and flow to the low detent', () => {
    const { container, root } = renderBody({ currentIndex: 0 })
    const surfaceStyle = (container.firstElementChild as HTMLElement).style
    const pulse = container.querySelector('.native-chat-effort-wave-pulse')

    expect(surfaceStyle.getPropertyValue('--effort-position')).toBe('5%')
    expect(surfaceStyle.getPropertyValue('--effort-pulse-length')).toBe('4.00')
    expect(pulse?.closest('g')?.getAttribute('clip-path')).toMatch(/^url\(#/)
    root.unmount()
  })

  it('lets the highest available effort level complete the whole waveform', () => {
    const { container, root } = renderBody({ currentIndex: CHOICES.length - 1 })
    const surfaceStyle = (container.firstElementChild as HTMLElement).style
    const clip = container.querySelector('.native-chat-effort-wave-clip')

    expect(surfaceStyle.getPropertyValue('--effort-position')).toBe('95%')
    expect(surfaceStyle.getPropertyValue('--effort-wave-position')).toBe('100%')
    expect(surfaceStyle.getPropertyValue('--effort-path-offset')).toBe('-100')
    expect(clip?.getAttribute('style')).toContain('scaleX(1)')
    root.unmount()
  })

  it('reveals a live preview and commits the nearest stop after dragging', async () => {
    await i18n.changeLanguage('en')
    const onCommitIndex = vi.fn()
    const { container, root } = renderBody({
      hasValue: false,
      currentIndex: 0,
      onCommitIndex
    })
    const slider = container.querySelector('[role="slider"]') as HTMLElement
    expect(container.querySelector('[aria-live="polite"]')?.textContent).toBe('Choose level')
    expect(slider.getAttribute('aria-valuetext')).toBe('Choose level')
    Object.defineProperty(slider, 'getBoundingClientRect', {
      value: () => ({ left: 0, width: 300 })
    })
    Object.defineProperty(slider, 'setPointerCapture', { value: vi.fn() })
    act(() => {
      slider.dispatchEvent(
        new PointerEvent('pointerdown', { bubbles: true, clientX: 260, pointerId: 1 })
      )
    })
    expect(slider.getAttribute('aria-valuetext')).toBe('High')
    act(() => {
      slider.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1 }))
    })
    expect(onCommitIndex).toHaveBeenCalledWith(2)
    root.unmount()
  })

  it('cancels a drag when pointer capture is lost', async () => {
    await i18n.changeLanguage('en')
    const onCommitIndex = vi.fn()
    const { container, root } = renderBody({ currentIndex: 1, onCommitIndex })
    const slider = container.querySelector('[role="slider"]') as HTMLElement
    Object.defineProperty(slider, 'getBoundingClientRect', {
      value: () => ({ left: 0, width: 300 })
    })
    Object.defineProperty(slider, 'setPointerCapture', { value: vi.fn() })
    act(() => {
      slider.dispatchEvent(
        new PointerEvent('pointerdown', { bubbles: true, clientX: 280, pointerId: 1 })
      )
    })
    expect(slider.getAttribute('data-dragging')).toBe('true')
    act(() => {
      slider.dispatchEvent(new PointerEvent('lostpointercapture', { bubbles: true, pointerId: 1 }))
    })
    act(() => {
      slider.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1 }))
    })

    expect(slider.getAttribute('data-dragging')).toBe('false')
    expect(onCommitIndex).not.toHaveBeenCalled()
    root.unmount()
  })

  it('rolls back an optimistic value that is never confirmed', async () => {
    await i18n.changeLanguage('en')
    vi.useFakeTimers()
    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    try {
      act(() => {
        root.render(
          <TooltipProvider>
            <NativeChatEffortSlider
              descriptor={{
                id: 'effort',
                label: 'Effort',
                category: 'thought_level',
                kind: { type: 'select', currentValue: 'medium', choices: CHOICES },
                valueSource: 'applied',
                settable: true
              }}
              disabled={false}
              saving={false}
              onValueChange={vi.fn().mockResolvedValue(true)}
            />
          </TooltipProvider>
        )
      })
      act(() => {
        container.querySelector('button')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      })
      const sliders = document.body.querySelectorAll('[role="slider"]')
      const slider = sliders.item(sliders.length - 1)
      act(() => {
        slider.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }))
      })
      expect(container.querySelector('button')?.textContent).toContain('High')

      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_000)
      })
      expect(container.querySelector('button')?.textContent).toContain('Medium')
    } finally {
      root.unmount()
      vi.useRealTimers()
    }
  })

  it('keeps the committed level visible through a transient unknown snapshot', async () => {
    await i18n.changeLanguage('en')
    const onCommitIndex = vi.fn()
    const { container, root } = renderBody({ currentIndex: 1, onCommitIndex })
    const slider = container.querySelector('[role="slider"]')!

    act(() => {
      slider.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }))
    })
    expect(slider.getAttribute('aria-valuetext')).toBe('High')

    act(() => {
      root.render(
        <TooltipProvider>
          <NativeChatEffortSliderBody
            choices={CHOICES}
            hasValue={false}
            currentIndex={0}
            interactive={false}
            saving={true}
            onCommitIndex={onCommitIndex}
          />
        </TooltipProvider>
      )
    })

    expect(slider.getAttribute('aria-valuetext')).toBe('High')
    expect(slider.getAttribute('aria-valuenow')).toBe('2')
    expect(container.querySelector('[aria-live="polite"]')?.textContent).toBe('High')
    root.unmount()
  })

  it('keeps the requested signal moving while saving and under reduced motion', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/renderer/src/assets/main.css'), 'utf8')

    expect(css).not.toMatch(
      /\.native-chat-effort-slider\[aria-disabled='true'\][\s\S]*?animation-play-state:\s*paused/
    )
    expect(css).not.toMatch(
      /@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.native-chat-effort-wave-pulse\s*\{[^}]*animation:\s*none/
    )
    expect(css).toMatch(
      /@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.native-chat-effort-fill\s*,[\s\S]*?transition-duration:\s*0\.01ms/
    )
    expect(css).toMatch(
      /stroke-dasharray:\s*var\(--effort-pulse-length\) var\(--effort-pulse-gap\)/
    )
    expect(css).not.toContain("[data-active='false'] .native-chat-effort-wave-pulse")
  })

  it('commits arrow-key steps', async () => {
    await i18n.changeLanguage('en')
    const onCommitIndex = vi.fn()
    const { container, root } = renderBody({ currentIndex: 1, onCommitIndex })
    const slider = container.querySelector('[role="slider"]')!
    act(() => {
      slider.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    })
    expect(onCommitIndex).toHaveBeenCalledWith(2)
    expect(slider.getAttribute('aria-valuetext')).toBe('High')
    act(() => {
      slider.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }))
    })
    expect(onCommitIndex).toHaveBeenCalledWith(1)
    root.unmount()
  })

  it('supports vertical arrows and Home/End navigation', async () => {
    await i18n.changeLanguage('en')
    const onCommitIndex = vi.fn()
    const { container, root } = renderBody({ currentIndex: 1, onCommitIndex })
    const slider = container.querySelector('[role="slider"]')!
    for (const key of ['ArrowUp', 'Home', 'End', 'ArrowDown']) {
      act(() => {
        slider.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
      })
    }
    expect(onCommitIndex.mock.calls.map(([index]) => index)).toEqual([2, 0, 2, 1])
    root.unmount()
  })

  it('keeps the waveform vivid while saving and dims truly unavailable controls', async () => {
    await i18n.changeLanguage('en')
    const onCommitIndex = vi.fn()
    const saving = renderBody({ interactive: false, saving: true, onCommitIndex })
    const unavailable = renderBody({ interactive: false, saving: false, onCommitIndex })
    const savingSlider = saving.container.querySelector('[role="slider"]')
    const unavailableSlider = unavailable.container.querySelector('[role="slider"]')

    expect(savingSlider?.getAttribute('aria-disabled')).toBe('true')
    expect(savingSlider?.getAttribute('tabindex')).toBe('-1')
    expect(savingSlider?.className).not.toContain('opacity-50')
    expect(unavailableSlider?.className).toContain('opacity-50')
    saving.root.unmount()
    unavailable.root.unmount()
  })
})
