// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { NativeChatTerminalAttention } from './NativeChatTerminalAttention'

describe('NativeChatTerminalAttention', () => {
  afterEach(cleanup)

  it('keeps Chat input mounted but hidden behind a clear terminal recovery action', () => {
    const onSwitchToTerminal = vi.fn()
    render(
      <NativeChatTerminalAttention
        attention="codex-hooks-review"
        agent="codex"
        onSwitchToTerminal={onSwitchToTerminal}
      >
        <textarea aria-label="Send a message" />
      </NativeChatTerminalAttention>
    )

    expect(screen.getByRole('alert')).toHaveTextContent('Codex needs a security review')
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Review new or changed hooks before Chat can continue.'
    )
    expect(screen.queryByRole('textbox', { name: 'Send a message' })).not.toBeInTheDocument()
    expect(document.querySelector('textarea[aria-label="Send a message"]')).not.toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Review in Terminal' }))
    expect(onSwitchToTerminal).toHaveBeenCalledOnce()
  })

  it('explains authentication recovery and preserves the mounted draft', () => {
    const onSwitchToTerminal = vi.fn()
    render(
      <NativeChatTerminalAttention
        attention="agent-auth-required"
        agent="claude"
        onSwitchToTerminal={onSwitchToTerminal}
      >
        <textarea aria-label="Send a message" defaultValue="unfinished draft" />
      </NativeChatTerminalAttention>
    )

    expect(screen.getByRole('alert')).toHaveTextContent('Claude session expired')
    expect(screen.getByRole('alert')).toHaveTextContent('Your draft is preserved')
    expect(document.querySelector('textarea')?.value).toBe('unfinished draft')
    fireEvent.click(screen.getByRole('button', { name: 'Log in in Terminal' }))
    expect(onSwitchToTerminal).toHaveBeenCalledOnce()
  })

  it('renders normal Chat input when the terminal needs no attention', () => {
    render(
      <NativeChatTerminalAttention attention={null} agent="codex">
        <textarea aria-label="Send a message" />
      </NativeChatTerminalAttention>
    )

    expect(screen.getByRole('textbox', { name: 'Send a message' })).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})
