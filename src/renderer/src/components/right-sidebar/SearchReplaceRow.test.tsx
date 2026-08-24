// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach } from 'vitest'
import { SearchReplaceRow } from './SearchReplaceRow'
import { vi } from 'vitest'

afterEach(cleanup)

const baseProps = {
  value: '',
  preserveCase: 'none' as const,
  useRegex: false,
  running: false,
  hasResults: true,
  confirmOccurrences: null,
  onValueChange: vi.fn(),
  onPreserveCaseToggle: vi.fn(),
  onRequestReplace: vi.fn(),
  onConfirmReplace: vi.fn(),
  onCancelConfirm: vi.fn()
}

describe('SearchReplaceRow', () => {
  it('disables Replace All without results or a replacement value', () => {
    const { rerender } = render(<SearchReplaceRow {...baseProps} hasResults={false} />)
    expect(
      (screen.getByText('Replace All').closest('button') as HTMLButtonElement).disabled
    ).toBe(true)

    rerender(<SearchReplaceRow {...baseProps} value='bar' />)
    expect(
      (screen.getByText('Replace All').closest('button') as HTMLButtonElement).disabled
    ).toBe(false)
  })

  it('hides the preserve-case toggle while regex mode is on', () => {
    const { rerender } = render(<SearchReplaceRow {...baseProps} useRegex={false} />)
    expect(screen.getByTitle('Preserve case')).toBeTruthy()
    rerender(<SearchReplaceRow {...baseProps} useRegex />)
    expect(screen.queryByTitle('Preserve case')).toBeNull()
  })

  it('swaps to an inline confirm strip and only replaces on confirm', () => {
    const onConfirm = vi.fn()
    const { rerender } = render(<SearchReplaceRow {...baseProps} value='bar' />)
    fireEvent.click(screen.getByText('Replace All'))
    expect(baseProps.onRequestReplace).toHaveBeenCalled()

    rerender(
      <SearchReplaceRow
        {...baseProps}
        value='bar'
        confirmOccurrences={7}
        onConfirmReplace={onConfirm}
      />
    )
    expect(screen.getByText('Replace 7 occurrence(s)?')).toBeTruthy()

    fireEvent.click(screen.getByText('Cancel'))
    expect(onConfirm).not.toHaveBeenCalled()
    expect(baseProps.onCancelConfirm).toHaveBeenCalled()
  })
})
