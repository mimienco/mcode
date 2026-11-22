// @vitest-environment happy-dom

import { cleanup, render, screen } from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({
    children,
    variant: _variant,
    size: _size,
    ...props
  }: {
    children: ReactNode
    variant?: string
    size?: string
  } & React.ButtonHTMLAttributes<HTMLButtonElement>) => <button {...props}>{children}</button>
}))

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: ReactNode }) => <div>{children}</div>
}))

vi.mock('./NativeChatSessionOptionPickers', () => ({
  NativeChatSessionOptionPickers: () => <div data-testid="session-option-pickers" />
}))

import { NativeChatComposerActions } from './NativeChatComposerActions'

afterEach(() => cleanup())

describe('NativeChatComposerActions', () => {
  it('places session option pickers immediately beside dictation', () => {
    render(
      <NativeChatComposerActions
        attachDisabled={false}
        dictationDisabled={false}
        sendDisabled={false}
        isWorking={false}
        isDictating={false}
        isDictationHoldMode={false}
        onAttach={vi.fn()}
        onDictationToggle={vi.fn()}
        onDictationHoldStart={vi.fn()}
        onDictationHoldEnd={vi.fn()}
        onSend={vi.fn()}
        sessionOptionsSurface={null}
        sessionOptionsSnapshot={[]}
      />
    )

    const pickers = screen.getByTestId('session-option-pickers')
    const dictation = screen.getByRole('button', { name: 'Start dictation' })
    expect(pickers.parentElement?.nextElementSibling?.contains(dictation)).toBe(true)
  })

  it('pins primary actions around a flexible run-profile column', () => {
    render(
      <NativeChatComposerActions
        attachDisabled={false}
        dictationDisabled={false}
        sendDisabled={false}
        isWorking={false}
        isDictating={false}
        isDictationHoldMode={false}
        onAttach={vi.fn()}
        onDictationToggle={vi.fn()}
        onDictationHoldStart={vi.fn()}
        onDictationHoldEnd={vi.fn()}
        onSend={vi.fn()}
        sessionOptionsSurface={null}
        sessionOptionsSnapshot={[]}
      />
    )

    const actions = document.querySelector('[data-native-chat-composer-actions]')
    expect(actions?.className).toContain('grid')
    expect(actions?.className).toContain('grid-cols-[auto_minmax(0,1fr)_auto_auto]')
  })

  it('uses one compact settings trigger in the split-width primary action row', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/renderer/src/assets/main.css'), 'utf8')

    expect(css).toMatch(
      /@container \(max-width: 35rem\)[\s\S]*?\[data-native-chat-composer-actions\]\s*\{[\s\S]*?grid-template-columns:\s*auto minmax\(0, 1fr\) auto auto[\s\S]*?row-gap:\s*0/
    )
    expect(css).toMatch(
      /@container \(max-width: 35rem\)[\s\S]*?\[data-native-chat-session-options\]\s*\{[\s\S]*?display:\s*none/
    )
    expect(css).toMatch(
      /@container \(max-width: 35rem\)[\s\S]*?\[data-native-chat-compact-options\]\s*\{[\s\S]*?display:\s*flex/
    )
  })
})
