// @vitest-environment happy-dom

import { cleanup, fireEvent, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AccountDraftTextField } from './AccountsPane'

// PERF-11: account text fields used to persist on every keystroke via
// onChange. The draft field must save once per confirmed change only.
afterEach(() => {
  cleanup()
})

describe('AccountDraftTextField single-commit', () => {
  it('saves nothing while typing and exactly once on blur', () => {
    const onSave = vi.fn()
    const { container } = render(<AccountDraftTextField value="abc" onSave={onSave} />)
    const input = container.querySelector('input')!
    input.focus()

    fireEvent.change(input, { target: { value: 'abcd' } })
    fireEvent.change(input, { target: { value: 'abcde' } })
    expect(onSave).not.toHaveBeenCalled()

    fireEvent.blur(input)
    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave).toHaveBeenLastCalledWith('abcde')
  })

  it('skips a blur that carries no change', () => {
    const onSave = vi.fn()
    const { container } = render(<AccountDraftTextField value="token" onSave={onSave} />)
    const input = container.querySelector('input')!
    input.focus()

    fireEvent.blur(input)
    expect(onSave).not.toHaveBeenCalled()
  })

  it('commits exactly once per Enter and never twice via the follow-up blur', () => {
    const onSave = vi.fn()
    const { container } = render(<AccountDraftTextField value="" onSave={onSave} />)
    const input = container.querySelector('input')!
    input.focus()

    fireEvent.change(input, { target: { value: 'wrk_123' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onSave).toHaveBeenCalledTimes(1)

    fireEvent.blur(input)
    expect(onSave).toHaveBeenCalledTimes(1)
  })

  it('restores the committed value on Escape without saving', () => {
    const onSave = vi.fn()
    const { container } = render(<AccountDraftTextField value="keep" onSave={onSave} />)
    const input = container.querySelector('input')!
    input.focus()

    fireEvent.change(input, { target: { value: 'draft' } })
    fireEvent.keyDown(input, { key: 'Escape' })
    expect(onSave).not.toHaveBeenCalled()
    expect(input.value).toBe('keep')
  })

  it('syncs an externally changed value when not focused', () => {
    const onSave = vi.fn()
    const { container, rerender } = render(<AccountDraftTextField value="one" onSave={onSave} />)
    const input = container.querySelector('input')!

    rerender(<AccountDraftTextField value="two" onSave={onSave} />)
    expect(input.value).toBe('two')

    fireEvent.blur(input)
    expect(onSave).not.toHaveBeenCalled()
  })

  it('keeps whitespace-sensitive values untouched (no trim)', () => {
    const onSave = vi.fn()
    const { container } = render(<AccountDraftTextField value="" onSave={onSave} />)
    const input = container.querySelector('input')!
    input.focus()

    fireEvent.change(input, { target: { value: ' Fe26.2** padded ' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onSave).toHaveBeenLastCalledWith(' Fe26.2** padded ')
  })
})
