// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { NativeChatMentionMenu, NativeChatPickerMenu } from './NativeChatAutocompleteMenus'
import type { ComposerAutocomplete } from './native-chat-composer-state'

function autocomplete(
  overrides: Partial<Extract<ComposerAutocomplete, { mode: 'slash' }>> = {}
): Extract<ComposerAutocomplete, { mode: 'slash' }> {
  return {
    mode: 'slash',
    query: '',
    triggerKey: '/:0',
    prefix: '/',
    grouped: true,
    commandsEnabled: true,
    skillsEnabled: true,
    items: [
      {
        kind: 'command',
        id: 'command:clear',
        name: 'clear',
        description: 'Clear history',
        skillCollision: false
      },
      {
        kind: 'skill',
        id: 'skill:browser',
        name: 'browser',
        description: 'Use a browser',
        sources: [{ sourceKind: 'repo', skillFilePath: '/repo/browser/SKILL.md' }]
      }
    ],
    skillStatus: 'ready',
    ...overrides
  }
}

describe('NativeChatPickerMenu', () => {
  afterEach(() => cleanup())

  it('renders grouped command and skill options with active-descendant ids', () => {
    const onChoose = vi.fn()
    render(
      <NativeChatPickerMenu
        autocomplete={autocomplete()}
        activeIndex={1}
        listboxId="picker"
        onChoose={onChoose}
        onRetry={vi.fn()}
      />
    )
    expect(screen.getByText('Commands')).toBeTruthy()
    expect(screen.getByText('Skills')).toBeTruthy()
    expect(screen.getByRole('option', { name: /browser/i }).getAttribute('aria-selected')).toBe(
      'true'
    )
    expect(screen.getByText('Project')).toBeTruthy()
  })

  it('completes a command on pointer down instead of dispatching it internally', () => {
    const onChoose = vi.fn()
    const value = autocomplete()
    render(
      <NativeChatPickerMenu
        autocomplete={value}
        activeIndex={0}
        listboxId="picker"
        onChoose={onChoose}
        onRetry={vi.fn()}
      />
    )
    fireEvent.pointerDown(screen.getByRole('option', { name: /clear/i }))
    expect(onChoose).toHaveBeenCalledWith(value.items[0])
  })

  it('keeps commands selectable while skills load', () => {
    render(
      <NativeChatPickerMenu
        autocomplete={autocomplete({ items: [autocomplete().items[0]], skillStatus: 'loading' })}
        activeIndex={0}
        listboxId="picker"
        onChoose={vi.fn()}
        onRetry={vi.fn()}
      />
    )
    expect(screen.getByRole('option', { name: /clear/i })).toBeTruthy()
    expect(screen.getAllByText('Loading skills...')).toHaveLength(2)
  })

  it('uses command-only empty copy for a picker without skill support', () => {
    render(
      <NativeChatPickerMenu
        autocomplete={autocomplete({
          grouped: false,
          items: [],
          skillsEnabled: false,
          skillStatus: 'ready'
        })}
        activeIndex={0}
        listboxId="picker"
        onChoose={vi.fn()}
        onRetry={vi.fn()}
      />
    )
    expect(screen.getAllByText('No matching commands')).toHaveLength(2)
  })

  it('announces a successful empty skill result distinctly from loading', () => {
    render(
      <NativeChatPickerMenu
        autocomplete={autocomplete({
          commandsEnabled: false,
          items: [],
          skillStatus: 'ready'
        })}
        activeIndex={0}
        listboxId="picker"
        onChoose={vi.fn()}
        onRetry={vi.fn()}
      />
    )
    expect(screen.getAllByText('No matching skills')).toHaveLength(2)
  })
})

describe('NativeChatMentionMenu', () => {
  afterEach(() => cleanup())

  it('renders type-aware icons and highlights fuzzy matches across the full path', () => {
    const { container } = render(
      <NativeChatMentionMenu
        query="srapp"
        files={[{ path: 'src/renderer/App.tsx', matchIndices: [0, 1, 13, 14, 15] }]}
        loading={false}
        loadError={null}
        truncated={false}
        activeIndex={0}
        listboxId="mentions"
        onChoose={vi.fn()}
      />
    )

    expect(screen.getByRole('option', { name: 'src/renderer/App.tsx' })).toBeTruthy()
    expect(container.querySelector('svg.lucide-file-code')).toBeTruthy()
    expect(
      Array.from(container.querySelectorAll('[data-native-chat-mention-match="true"]'))
        .map((element) => element.textContent)
        .join('')
    ).toBe('srApp')
  })

  it('chooses the row that was clicked instead of the keyboard-active row', () => {
    const onChoose = vi.fn()
    render(
      <NativeChatMentionMenu
        query="app"
        files={[
          { path: 'src/App.tsx', matchIndices: [4, 5, 6] },
          { path: 'test/App.test.tsx', matchIndices: [5, 6, 7] }
        ]}
        loading={false}
        loadError={null}
        truncated={false}
        activeIndex={0}
        listboxId="mentions"
        onChoose={onChoose}
      />
    )

    fireEvent.pointerDown(screen.getByRole('option', { name: 'test/App.test.tsx' }))
    expect(onChoose).toHaveBeenCalledWith('test/App.test.tsx')
  })

  it('announces result count and exposes truncation instead of silently hiding matches', () => {
    render(
      <NativeChatMentionMenu
        query="app"
        files={[{ path: 'src/App.tsx', matchIndices: [4, 5, 6] }]}
        loading={false}
        loadError={null}
        truncated
        activeIndex={0}
        listboxId="mentions"
        onChoose={vi.fn()}
      />
    )

    expect(screen.getAllByText(/More matches available/)).toHaveLength(2)
    expect(screen.getByText(/Matching files: 1.*More matches available/)).toBeTruthy()
    expect(screen.getByRole('option').className).toContain('pointer-coarse:min-h-11')
  })
})
