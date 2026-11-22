// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => {
  const state = {
    settings: {
      savedSnippets: [
        {
          id: 'review-diff',
          name: 'Review Git changes',
          text: 'Review the current Git diff.'
        }
      ]
    },
    updateSettings: vi.fn()
  }
  return {
    state,
    useAppStore: (selector: (value: typeof state) => unknown) => selector(state)
  }
})

vi.mock('@/store', () => ({ useAppStore: mocks.useAppStore }))
vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string, values?: Record<string, string>) =>
    values
      ? Object.entries(values).reduce(
          (text, [key, value]) => text.replace(`{{${key}}}`, value),
          fallback
        )
      : fallback
}))
vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: ReactNode }) => <>{children}</>
}))
vi.mock('@/components/confirmation-dialog-context', () => ({
  useConfirmationDialog: () => vi.fn()
}))

import { TabBarSnippetsButton } from './TabBarSnippetsButton'

afterEach(cleanup)

describe('TabBarSnippetsButton', () => {
  it('opens a generic card menu that names both drop destinations', () => {
    render(<TabBarSnippetsButton />)

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Open snippets' }), {
      button: 0,
      ctrlKey: false
    })

    expect(screen.getByText('Snippet cards')).toBeTruthy()
    expect(screen.getByText('Drag a card into Chat or Terminal')).toBeTruthy()
    expect(screen.getByText('Review Git changes')).toBeTruthy()
    expect(screen.queryByText('Append Enter')).toBeNull()
  })
})
