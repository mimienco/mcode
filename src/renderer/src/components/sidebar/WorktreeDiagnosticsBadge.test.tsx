import type React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'

import { WorktreeDiagnosticsBadgeView } from './WorktreeDiagnosticsBadge'

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => children,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => children,
  TooltipContent: ({ children }: { children: React.ReactNode }) => <span>{children}</span>
}))

describe('WorktreeDiagnosticsBadge', () => {
  it('renders the objective failure count on an affected worktree card', () => {
    const markup = renderToStaticMarkup(
      <WorktreeDiagnosticsBadgeView failed={3} onOpen={() => {}} />
    )

    expect(markup).toContain('3 failed checks')
    expect(markup).toContain('>3<')
  })
})
