// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { NativeChatTerminalNotice } from './NativeChatTerminalNotice'

describe('NativeChatTerminalNotice', () => {
  afterEach(cleanup)

  it('shows a completed update and links to its changelog', () => {
    render(
      <NativeChatTerminalNotice
        notice={{
          kind: 'update-complete',
          features: 10,
          bugfixes: 34,
          otherChanges: 17,
          changelogUrl: 'https://code.claude.com/docs/en/changelog'
        }}
      />
    )

    expect(screen.getByRole('status')).toHaveTextContent('Claude Code updated')
    expect(screen.getByRole('status')).toHaveTextContent(
      '10 features, 34 bugfixes, and 17 other changes.'
    )
    expect(screen.getByRole('link', { name: /Changelog/ })).toHaveAttribute(
      'href',
      'https://code.claude.com/docs/en/changelog'
    )
  })

  it('runs an available update with one click', () => {
    const onUpdate = vi.fn()
    render(
      <NativeChatTerminalNotice
        notice={{ kind: 'update-available', command: 'claude update' }}
        onUpdate={onUpdate}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Update Claude' }))
    expect(onUpdate).toHaveBeenCalledWith('claude update')
  })
})
