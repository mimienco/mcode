// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import type * as ReactModule from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { SessionOptionDescriptor } from '../../../../shared/native-chat-session-options'

const confirmationMocks = vi.hoisted(() => ({
  confirm: vi.fn().mockResolvedValue(true)
}))

vi.mock('@/components/confirmation-dialog-context', () => ({
  useConfirmationDialog: () => confirmationMocks.confirm
}))

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string, values?: Record<string, string | number>) => {
    if (!values) {
      return fallback
    }
    return Object.entries(values).reduce(
      (text, [name, value]) => text.replaceAll(`{{${name}}}`, String(value)),
      fallback
    )
  }
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props}>{children}</button>
  )
}))

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>
}))

vi.mock('./NativeChatEffortSlider', () => ({
  NativeChatEffortSlider: ({
    descriptor,
    disabled,
    saving,
    onValueChange
  }: {
    descriptor: SessionOptionDescriptor & { kind: { type: 'select' } }
    disabled: boolean
    saving: boolean
    onValueChange: (value: string) => Promise<boolean>
  }) => (
    <button
      aria-label={`Effort slider ${descriptor.kind.currentValue ?? 'unset'}`}
      data-disabled={disabled || undefined}
      data-saving={saving || undefined}
      onClick={() => {
        if (descriptor.settable) {
          void onValueChange(descriptor.kind.choices[0]?.value ?? '')
        }
      }}
    >
      {descriptor.kind.currentValue ?? 'Effort'}
    </button>
  )
}))

vi.mock('@/components/ui/dropdown-menu', () => {
  const React = require('react') as typeof ReactModule
  return {
    DropdownMenu: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    DropdownMenuTrigger: ({
      children,
      disabled
    }: {
      children: React.ReactNode
      disabled?: boolean
    }) => <div data-disabled={disabled || undefined}>{children}</div>,
    DropdownMenuContent: ({
      children,
      side,
      sideOffset,
      collisionPadding
    }: {
      children: React.ReactNode
      side?: string
      sideOffset?: number
      collisionPadding?: number
    }) => (
      <div
        data-testid="session-option-menu"
        data-side={side}
        data-side-offset={sideOffset}
        data-collision-padding={collisionPadding}
      >
        {children}
      </div>
    ),
    DropdownMenuArrow: ({ className }: { className?: string }) => (
      <svg data-testid="session-option-arrow" className={className} />
    ),
    DropdownMenuLabel: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    DropdownMenuSeparator: () => <hr />,
    DropdownMenuItem: ({
      children,
      disabled,
      onSelect
    }: React.ButtonHTMLAttributes<HTMLButtonElement> & { onSelect?: () => void }) => (
      <button disabled={disabled} onClick={() => onSelect?.()}>
        {children}
      </button>
    ),
    // Why: exercises value binding + onValueChange contract the real Radix
    // group provides; selected value is exposed via data-radio-value.
    DropdownMenuRadioGroup: ({
      children,
      value,
      onValueChange
    }: {
      children: React.ReactNode
      value?: string
      onValueChange?: (value: string) => void
    }) => (
      <div
        role="radiogroup"
        data-radio-value={value ?? ''}
        data-on-value-change={onValueChange ? '1' : '0'}
      >
        {React.Children.map(children, (child) => {
          if (!React.isValidElement(child)) {
            return child
          }
          const props = child.props as {
            value?: string
            disabled?: boolean
            children?: React.ReactNode
            'aria-current'?: 'true'
          }
          if (props.value === undefined) {
            return child
          }
          const selected = props.value !== undefined && props.value === value
          return (
            <button
              key={props.value}
              role="radio"
              aria-checked={selected}
              aria-current={props['aria-current']}
              disabled={props.disabled}
              data-value={props.value}
              data-state={selected ? 'checked' : 'unchecked'}
              onClick={() => {
                if (props.value !== undefined) {
                  onValueChange?.(props.value)
                }
              }}
            >
              {props.children}
            </button>
          )
        })}
      </div>
    ),
    DropdownMenuRadioItem: ({
      children,
      disabled,
      value
    }: React.ButtonHTMLAttributes<HTMLButtonElement> & { value: string }) => (
      // Why: parent RadioGroup mock reads `value` via Children.map — keep it on
      // props even though native span has no value attribute.
      <span
        data-radio-item
        data-disabled={disabled || undefined}
        {...({ value } as Record<string, string>)}
      >
        {children}
      </span>
    )
  }
})

import { NativeChatSessionOptionPickers } from './NativeChatSessionOptionPickers'

const surface = {
  getSnapshot: vi.fn(() => []),
  setOption: vi.fn(),
  invokeAction: vi.fn(),
  subscribe: vi.fn(() => vi.fn())
}

function model(overrides: Partial<SessionOptionDescriptor> = {}): SessionOptionDescriptor {
  return {
    id: 'model',
    label: 'Model',
    category: 'model',
    kind: {
      type: 'select',
      currentValue: 'opus',
      choices: [
        { value: 'opus', label: 'Opus 4.8' },
        { value: 'sonnet', label: 'Sonnet 5' }
      ]
    },
    valueSource: 'applied',
    settable: true,
    ...overrides
  }
}

const effort: SessionOptionDescriptor = {
  id: 'effort',
  label: 'Effort',
  category: 'thought_level',
  kind: {
    type: 'select',
    currentValue: 'high',
    choices: [
      { value: 'low', label: 'Low' },
      { value: 'high', label: 'High' }
    ]
  },
  valueSource: 'applied',
  settable: true
}

const fast: SessionOptionDescriptor = {
  id: 'fastMode',
  label: 'Fast mode',
  category: 'mode',
  kind: { type: 'boolean', currentValue: true },
  valueSource: 'applied',
  settable: true
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  confirmationMocks.confirm.mockReset().mockResolvedValue(true)
})

describe('NativeChatSessionOptionPickers', () => {
  it('renders permissions and Plan/Build as separate composer controls', () => {
    render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[
          model(),
          {
            id: 'permission',
            label: 'Permissions',
            category: 'permission',
            kind: {
              type: 'select',
              currentValue: 'auto-edits',
              choices: [
                { value: 'ask', label: 'Ask approval' },
                { value: 'auto-edits', label: 'Auto-accept edits' },
                {
                  value: 'bypass',
                  label: 'Bypass permissions',
                  disabled: true,
                  disabledReason: 'Blocked by policy.'
                }
              ]
            },
            valueSource: 'reported',
            applyScope: 'current-session',
            settable: true
          },
          {
            id: 'interaction-mode',
            label: 'Work mode',
            category: 'interaction_mode',
            kind: {
              type: 'select',
              currentValue: 'build',
              choices: [
                { value: 'plan', label: 'Plan' },
                { value: 'build', label: 'Build' }
              ]
            },
            valueSource: 'reported',
            applyScope: 'current-session',
            settable: true
          }
        ]}
        isWorking={false}
      />
    )

    expect(screen.getByRole('button', { name: 'Permissions · Applies to this task Auto-accept edits' })).not.toBeNull()
    expect(screen.getByRole('button', { name: 'Work mode · Applies to this task Build' })).not.toBeNull()
    expect(
      (screen.getByRole('radio', { name: /Bypass permissions/ }) as HTMLButtonElement).disabled
    ).toBe(true)
  })

  it('requires explicit confirmation before applying bypass', async () => {
    confirmationMocks.confirm.mockResolvedValue(false)
    const setOption = vi.fn().mockResolvedValue({ snapshot: [] })
    render(
      <NativeChatSessionOptionPickers
        surface={{ ...surface, setOption }}
        snapshot={[
          model(),
          {
            id: 'permission',
            label: 'Permissions',
            category: 'permission',
            kind: {
              type: 'select',
              currentValue: 'ask',
              choices: [
                { value: 'ask', label: 'Ask approval' },
                { value: 'bypass', label: 'Bypass permissions' }
              ]
            },
            valueSource: 'default',
            applyScope: 'next-session',
            settable: true
          }
        ]}
        isWorking={false}
      />
    )

    screen.getByRole('radio', { name: 'Bypass permissions' }).click()
    await waitFor(() => expect(confirmationMocks.confirm).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(setOption).not.toHaveBeenCalled())
  })

  it('prefers collision-aware upward placement for model and option menus', () => {
    render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[model(), effort]}
        isWorking={false}
      />
    )

    const menus = screen.getAllByTestId('session-option-menu')
    expect(menus).toHaveLength(1)
    for (const menu of menus) {
      expect(menu.getAttribute('data-side')).toBe('top')
      expect(menu.getAttribute('data-side-offset')).toBe('8')
      expect(menu.getAttribute('data-collision-padding')).toBe('8')
    }
    expect(screen.getAllByTestId('session-option-arrow')).toHaveLength(1)
    expect(screen.getByTestId('session-option-arrow').classList).toContain(
      'native-chat-picker-arrow'
    )
  })

  it('renders model and joined option labels, and hides an empty options pill', () => {
    const { rerender } = render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[model(), effort, fast]}
        isWorking={false}
      />
    )
    expect(screen.getByRole('button', { name: 'Model Opus 4.8' }).textContent).toContain('Opus 4.8')
    expect(screen.getByRole('button', { name: 'Model Opus 4.8' }).textContent).not.toContain(
      'Model:'
    )
    expect(screen.getByRole('button', { name: 'Effort slider high' }).textContent).toContain('high')
    expect(screen.getByRole('button', { name: 'Session options Fast' }).textContent).toContain(
      'Fast'
    )
    expect(
      screen
        .getByRole('button', { name: 'Effort slider high' })
        .compareDocumentPosition(screen.getByRole('button', { name: 'Model Opus 4.8' })) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).not.toBe(0)

    rerender(
      <NativeChatSessionOptionPickers surface={surface} snapshot={[model()]} isWorking={false} />
    )
    expect(screen.queryByRole('button', { name: /Effort/ })).toBeNull()
  })

  it('names a lone unknown effort control explicitly', () => {
    render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[
          model(),
          { ...effort, kind: { ...effort.kind, currentValue: undefined }, valueSource: 'unknown' }
        ]}
        isWorking={false}
      />
    )

    expect(screen.getByRole('button', { name: 'Effort slider unset' }).textContent).toContain(
      'Effort'
    )
  })

  it('keeps unresolved effort visible but non-interactive until a model is chosen', () => {
    const setOption = vi.fn().mockResolvedValue({ snapshot: [] })
    render(
      <NativeChatSessionOptionPickers
        surface={{ ...surface, setOption }}
        snapshot={[
          model(),
          {
            ...effort,
            kind: { ...effort.kind, currentValue: undefined },
            valueSource: 'unknown',
            settable: false,
            disabledReason: 'choose-model-first'
          }
        ]}
        isWorking={false}
      />
    )

    const trigger = screen.getByRole('button', { name: 'Effort slider unset' })
    expect(trigger.getAttribute('data-disabled')).toBeNull()
    trigger.click()
    expect(setOption).not.toHaveBeenCalled()
  })

  it('disables both picker triggers while the agent is working', () => {
    render(
      <NativeChatSessionOptionPickers surface={surface} snapshot={[model(), effort]} isWorking />
    )
    expect(
      screen
        .getByRole('button', { name: 'Model Opus 4.8' })
        .parentElement?.getAttribute('data-disabled')
    ).toBe('true')
    expect(
      screen.getByRole('button', { name: 'Effort slider high' }).getAttribute('data-disabled')
    ).toBe('true')
  })

  it('does not duplicate titles for unknown values or misname generic controls', () => {
    const { rerender } = render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[
          model({
            kind: { type: 'select', choices: [] },
            valueSource: 'unknown'
          }),
          { ...effort, kind: { ...effort.kind, currentValue: undefined }, valueSource: 'unknown' }
        ]}
        isWorking={false}
      />
    )
    expect(screen.getByRole('button', { name: 'Model' }).textContent).toContain('Model')
    expect(screen.getByRole('button', { name: 'Model' }).textContent).not.toContain('Model: Model')
    expect(screen.getByRole('button', { name: 'Effort slider unset' }).textContent).toBe('Effort')

    rerender(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[model(), fast]}
        isWorking={false}
      />
    )
    expect(screen.getByRole('button', { name: 'Session options Fast' }).textContent).toContain(
      'Fast'
    )
    expect(screen.queryByRole('button', { name: /^Effort/ })).toBeNull()
  })

  it('shows the unconfirmed hint for dispatched values', () => {
    render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[model({ valueSource: 'dispatched' })]}
        isWorking={false}
      />
    )
    expect(screen.getByText('Sent to the agent — not confirmed')).not.toBeNull()
  })

  it('does not present passive model rows as selectable in a legacy session', async () => {
    const invokeAction = vi.fn().mockResolvedValue({ snapshot: [] })
    const liveSurface = { ...surface, invokeAction }
    render(
      <NativeChatSessionOptionPickers
        surface={liveSurface}
        snapshot={[
          model({
            kind: {
              type: 'select',
              choices: [
                { value: 'gpt-5.5', label: 'GPT-5.5' },
                { value: 'gpt-5.2-codex', label: 'GPT-5.2 Codex' }
              ]
            },
            valueSource: 'unknown',
            action: { type: 'agent-picker' }
          })
        ]}
        isWorking={false}
      />
    )
    const action = screen.getByRole('button', { name: 'Choose model in Terminal…' })
    expect(
      screen.getByText(
        'Direct model switching is unavailable for this session. Choose in Terminal or start a new Codex session.'
      )
    ).not.toBeNull()
    expect(screen.queryByText('Known models')).toBeNull()
    expect(screen.queryByText('GPT-5.5')).toBeNull()
    expect(screen.queryByText('GPT-5.2 Codex')).toBeNull()
    action.click()
    await waitFor(() => expect(invokeAction).toHaveBeenCalledWith('model'))
  })

  it('selects a model directly when the live descriptor supports values', async () => {
    const setOption = vi.fn().mockResolvedValue({ snapshot: [] })
    render(
      <NativeChatSessionOptionPickers
        surface={{ ...surface, setOption }}
        snapshot={[
          model({
            kind: {
              type: 'select',
              currentValue: 'gpt-5.5',
              choices: [
                {
                  value: 'gpt-5.5',
                  label: 'GPT-5.5',
                  description: 'Strong reasoning for real-world work'
                },
                {
                  value: 'gpt-5.6-sol',
                  label: 'GPT-5.6 Sol',
                  description: 'Best for complex coding and agentic work'
                }
              ]
            },
            valueSource: 'reported',
            action: undefined
          })
        ]}
        isWorking={false}
      />
    )

    const currentModel = screen.getByRole('radio', {
      name: 'GPT-5.5 Strong reasoning for real-world work'
    })
    const alternative = screen.getByRole('radio', {
      name: 'GPT-5.6 Sol Best for complex coding and agentic work'
    })
    expect(currentModel.getAttribute('aria-current')).toBe('true')
    expect(
      currentModel.compareDocumentPosition(alternative) & Node.DOCUMENT_POSITION_FOLLOWING
    ).not.toBe(0)
    expect(screen.getByText('Available choices')).not.toBeNull()

    alternative.click()

    await waitFor(() => expect(setOption).toHaveBeenCalledWith('model', 'gpt-5.6-sol'))
  })

  it('uses a Toggle action for unknown flip-only options via invokeAction', async () => {
    const invokeAction = vi.fn().mockResolvedValue({ snapshot: [] })
    const setOption = vi.fn().mockResolvedValue({ snapshot: [] })
    const liveSurface = { ...surface, setOption, invokeAction }
    render(
      <NativeChatSessionOptionPickers
        surface={liveSurface}
        snapshot={[
          model(),
          {
            ...fast,
            kind: { type: 'boolean' },
            valueSource: 'unknown',
            action: { type: 'toggle-command' }
          }
        ]}
        isWorking={false}
      />
    )
    expect(screen.getByText('Toggle fast mode')).not.toBeNull()
    expect(screen.queryByText('On')).toBeNull()
    expect(screen.queryByText('Off')).toBeNull()
    screen.getByText('Toggle fast mode').click()
    await waitFor(() => expect(invokeAction).toHaveBeenCalledWith('fastMode'))
    expect(setOption).not.toHaveBeenCalled()
  })

  it('uses On/Off radios for known boolean options without inventing a selection', async () => {
    const setOption = vi.fn().mockResolvedValue({ snapshot: [] })
    const liveSurface = { ...surface, setOption }
    const { rerender } = render(
      <NativeChatSessionOptionPickers
        surface={liveSurface}
        snapshot={[
          model(),
          {
            ...fast,
            kind: { type: 'boolean', currentValue: true },
            valueSource: 'applied',
            action: undefined
          }
        ]}
        isWorking={false}
      />
    )
    expect(screen.queryByText('Toggle fast mode')).toBeNull()
    const onRadio = screen.getByRole('radio', { name: 'On' })
    expect(onRadio.getAttribute('data-state')).toBe('checked')
    expect(onRadio.getAttribute('aria-checked')).toBe('true')
    const fastGroup = onRadio.parentElement
    expect(fastGroup?.getAttribute('data-radio-value')).toBe('on')
    expect(fastGroup?.getAttribute('data-on-value-change')).toBe('1')
    screen.getByRole('radio', { name: 'Off' }).click()
    await waitFor(() => expect(setOption).toHaveBeenCalledWith('fastMode', false))

    setOption.mockClear()
    rerender(
      <NativeChatSessionOptionPickers
        surface={liveSurface}
        snapshot={[
          model(),
          {
            id: 'thinking',
            label: 'Thinking',
            category: 'mode',
            kind: { type: 'boolean' },
            valueSource: 'unknown',
            settable: true
          }
        ]}
        isWorking={false}
      />
    )
    // Unknown composed boolean: hint + radios present, nothing pre-selected.
    expect(screen.getByText('Current value unknown — pick On or Off')).not.toBeNull()
    const thinkingGroup = screen.getByRole('radio', { name: 'On' }).parentElement
    expect(thinkingGroup?.getAttribute('data-radio-value')).toBe('')
    screen.getByRole('radio', { name: 'Off' }).click()
    await waitFor(() => expect(setOption).toHaveBeenCalledWith('thinking', false))
  })

  it('does not show unconfirmed for applied flip-only booleans', () => {
    render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[
          model(),
          {
            ...fast,
            kind: { type: 'boolean', currentValue: true },
            // Why: flip-only tracks as applied — never a healable dispatched state.
            valueSource: 'applied'
          }
        ]}
        isWorking={false}
      />
    )
    expect(screen.queryByText('Sent to the agent — not confirmed')).toBeNull()
  })

  it('shows unconfirmed for confirmable dispatched booleans', () => {
    render(
      <NativeChatSessionOptionPickers
        surface={surface}
        snapshot={[
          model(),
          {
            id: 'thinking',
            label: 'Thinking',
            category: 'mode',
            kind: { type: 'boolean', currentValue: true },
            valueSource: 'dispatched',
            settable: true
          }
        ]}
        isWorking={false}
      />
    )
    expect(screen.getByText('Sent to the agent — not confirmed')).not.toBeNull()
  })
})
