// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { SavedSnippetDialog } from './SavedSnippetDialog'

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

afterEach(cleanup)

describe('SavedSnippetDialog', () => {
  it('edits a generic text card without terminal or agent settings', () => {
    render(<SavedSnippetDialog mode="add" onClose={vi.fn()} onSave={vi.fn()} />)

    expect(screen.getByRole('dialog')).toBeTruthy()
    expect(screen.getByLabelText('Name')).toBeTruthy()
    expect(screen.getByLabelText('Text')).toBeTruthy()
    expect(screen.queryByText('Terminal')).toBeNull()
    expect(screen.queryByText('Agent')).toBeNull()
    expect(screen.queryByText(/Append Enter/i)).toBeNull()
  })

  it('saves the card name and text', () => {
    const onSave = vi.fn()
    render(<SavedSnippetDialog mode="add" onClose={vi.fn()} onSave={onSave} />)

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Git status' } })
    fireEvent.change(screen.getByLabelText('Text'), { target: { value: 'git status' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save card' }))

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Git status', text: 'git status' })
    )
  })
})
