// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ShowEarlierInWindowButton } from './use-windowed-transcript'

afterEach(cleanup)

describe('ShowEarlierInWindowButton', () => {
  it('does not render when no transcript messages are hidden', () => {
    render(<ShowEarlierInWindowButton count={0} onClick={vi.fn()} />)

    expect(screen.queryByRole('button', { name: /Show earlier messages/ })).not.toBeInTheDocument()
  })

  it('renders the hidden count and expands the transcript when messages are hidden', () => {
    const onClick = vi.fn()
    render(<ShowEarlierInWindowButton count={3} onClick={onClick} />)

    fireEvent.click(screen.getByRole('button', { name: 'Show earlier messages (3)' }))

    expect(onClick).toHaveBeenCalledOnce()
  })
})
