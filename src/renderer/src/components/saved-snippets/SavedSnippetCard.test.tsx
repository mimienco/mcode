// @vitest-environment happy-dom

import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { QUICK_COMMAND_DRAG_MIME } from '@/lib/quick-command-drag-data'
import { SavedSnippetCard } from './SavedSnippetCard'

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string, values?: Record<string, string>) =>
    values
      ? Object.entries(values).reduce(
          (text, [key, value]) => text.replace(`{{${key}}}`, value),
          fallback
        )
      : fallback
}))

afterEach(cleanup)

describe('SavedSnippetCard', () => {
  it('publishes reusable plain text for either Chat or Terminal', () => {
    const values = new Map<string, string>()
    const dataTransfer = {
      effectAllowed: 'uninitialized',
      dropEffect: 'none',
      setData: vi.fn((type: string, value: string) => values.set(type, value))
    }
    render(
      <SavedSnippetCard
        snippet={{ id: 'git-status', name: 'Git status', text: 'git status' }}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onDragStarted={vi.fn()}
        onDragFinished={vi.fn()}
      />
    )

    const event = new Event('dragstart', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'dataTransfer', { value: dataTransfer })
    screen.getByRole('group', { name: 'Git status' }).dispatchEvent(event)

    expect(dataTransfer.effectAllowed).toBe('copy')
    expect(values.get('text/plain')).toBe('git status')
    expect(JSON.parse(values.get(QUICK_COMMAND_DRAG_MIME) ?? '{}')).toEqual({
      version: 1,
      label: 'Git status',
      text: 'git status'
    })
  })
})
