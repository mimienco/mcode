// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { NativeChatFileReferenceChip } from './NativeChatFileReferenceChip'
import { TooltipProvider } from '@/components/ui/tooltip'

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

describe('NativeChatFileReferenceChip', () => {
  afterEach(() => cleanup())

  function renderChip(path: string, onRemove = vi.fn()): ReturnType<typeof render> {
    return render(
      <TooltipProvider>
        <NativeChatFileReferenceChip reference={{ id: 'ref-1', path }} onRemove={onRemove} />
      </TooltipProvider>
    )
  }

  it('renders the selected relative path with its file-type icon', () => {
    const { container } = renderChip('src/App.tsx')

    expect(screen.getByText('@src/App.tsx')).toBeTruthy()
    expect(container.querySelector('svg.lucide-file-code')).toBeTruthy()
    expect(container.querySelector('[data-native-chat-file-reference]')).toBeTruthy()
  })

  it('removes only the selected reference', () => {
    const onRemove = vi.fn()
    renderChip('docs/Guide.md', onRemove)

    fireEvent.click(screen.getByRole('button', { name: 'Remove attachment' }))
    expect(onRemove).toHaveBeenCalledWith('ref-1')
  })
})
