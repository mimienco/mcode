import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'

import { WorkspaceDiagnosticsList } from './WorkspaceProblemsPanel'

describe('WorkspaceDiagnosticsList', () => {
  it('shows objective failure counts and exposes source locations as buttons', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceDiagnosticsList
        run={{
          tool: 'vitest',
          status: 'failed',
          failed: 1,
          passed: 19,
          total: 20,
          updatedAt: 1,
          diagnostics: [
            {
              id: 'failure-1',
              kind: 'test',
              severity: 'error',
              filePath: 'src/cart/cart.test.ts',
              line: 142,
              column: 7,
              title: 'cart totals > applies discounts',
              message: 'expected 12 to be 10',
              code: null
            }
          ]
        }}
        onClear={vi.fn()}
        onOpenDiagnostic={vi.fn()}
      />
    )

    expect(markup).toContain('1 failed')
    expect(markup).toContain('19 passed')
    expect(markup).toContain('cart totals &gt; applies discounts')
    expect(markup).toContain('Line 142:7')
    expect(markup).toContain('<button')
  })
})
