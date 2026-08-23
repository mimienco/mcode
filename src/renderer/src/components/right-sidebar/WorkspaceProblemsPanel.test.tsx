import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'

import { WorkspaceDiagnosticsList } from './WorkspaceProblemsPanel'
import { workspaceDiagnosticsLiveStatus } from './workspace-problems-panel-model'

describe('WorkspaceDiagnosticsList', () => {
  it('shows objective failure counts and exposes source locations as buttons', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceDiagnosticsList
        run={{
          tools: ['vitest'],
          runsByTool: {},
          outputPtyId: 'pty-tests',
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
    expect(markup).toContain(
      'cart totals &gt; applies discounts. expected 12 to be 10. src/cart/cart.test.ts, Line 142:7'
    )
    expect(markup).toContain('<button')
  })

  it('does not present a failed check without locations as healthy', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceDiagnosticsList
        run={{
          tools: ['vitest'],
          runsByTool: {},
          outputPtyId: 'pty-tests',
          status: 'failed',
          failed: 0,
          passed: 2,
          total: 2,
          diagnostics: [],
          updatedAt: 1
        }}
        onClear={() => {}}
        onOpenDiagnostic={() => {}}
        onShowOutput={() => {}}
        availableOutputPtyIds={new Set(['pty-tests'])}
      />
    )

    expect(markup).toContain('Check failed')
    expect(markup).toContain('without a source location')
    expect(markup).toContain('Show output')
    expect(markup).not.toContain('All 2 tests passed')
  })

  it('surfaces output for a locationless failure alongside located diagnostics', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceDiagnosticsList
        run={{
          tools: ['vitest', 'typescript'],
          runsByTool: {
            vitest: {
              tool: 'vitest',
              ptyId: 'pty-tests',
              status: 'failed',
              failed: 1,
              passed: 0,
              total: 1,
              diagnostics: [],
              updatedAt: 1
            },
            typescript: {
              tool: 'typescript',
              ptyId: 'pty-types',
              status: 'failed',
              failed: 1,
              passed: 0,
              total: 1,
              diagnostics: [
                {
                  id: 'type-error',
                  kind: 'compiler',
                  severity: 'error',
                  filePath: 'src/cart.ts',
                  line: 3,
                  column: 2,
                  title: 'TS2322',
                  message: 'Type mismatch',
                  code: 'TS2322'
                }
              ],
              updatedAt: 1
            }
          },
          outputPtyId: 'pty-types',
          status: 'failed',
          failed: 2,
          passed: 0,
          total: 2,
          diagnostics: [
            {
              id: 'type-error',
              kind: 'compiler',
              severity: 'error',
              filePath: 'src/cart.ts',
              line: 3,
              column: 2,
              title: 'TS2322',
              message: 'Type mismatch',
              code: 'TS2322'
            }
          ],
          updatedAt: 1
        }}
        onClear={() => {}}
        onOpenDiagnostic={() => {}}
        onShowOutput={() => {}}
        availableOutputPtyIds={new Set(['pty-tests'])}
      />
    )

    expect(markup).toContain('without a source location')
    expect(markup).toContain('Show Vitest output')
    expect(markup).toContain('src/cart.ts')
  })

  it('does not render an output action after its terminal is unavailable', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceDiagnosticsList
        run={{
          tools: ['vitest'],
          runsByTool: {},
          outputPtyId: 'closed-pty',
          status: 'failed',
          failed: 1,
          passed: 0,
          total: 1,
          diagnostics: [],
          updatedAt: 1
        }}
        onClear={() => {}}
        onOpenDiagnostic={() => {}}
        onShowOutput={() => {}}
      />
    )

    expect(markup).toContain('without a source location')
    expect(markup).not.toContain('Show output')
  })

  it('offers output when only some failures in a tool have source locations', () => {
    const diagnostic = {
      id: 'located-failure',
      kind: 'test' as const,
      severity: 'error' as const,
      filePath: 'src/cart.test.ts',
      line: 8,
      column: 2,
      title: 'cart fails',
      message: 'wrong total',
      code: null
    }
    const markup = renderToStaticMarkup(
      <WorkspaceDiagnosticsList
        run={{
          tools: ['vitest'],
          runsByTool: {
            vitest: {
              tool: 'vitest',
              ptyId: 'pty-tests',
              status: 'failed',
              failed: 2,
              passed: 3,
              total: 5,
              diagnostics: [diagnostic],
              updatedAt: 1
            }
          },
          outputPtyId: 'pty-tests',
          status: 'failed',
          failed: 2,
          passed: 3,
          total: 5,
          diagnostics: [diagnostic],
          updatedAt: 1
        }}
        onClear={() => {}}
        onOpenDiagnostic={() => {}}
        onShowOutput={() => {}}
        availableOutputPtyIds={new Set(['pty-tests'])}
      />
    )

    expect(markup).toContain('Show Vitest output')
    expect(markup).toContain('cart fails')
  })

  it('announces successful and warning count changes', () => {
    expect(
      workspaceDiagnosticsLiveStatus({
        tools: ['typescript'],
        runsByTool: {},
        outputPtyId: null,
        status: 'passed',
        failed: 0,
        passed: 12,
        total: 13,
        diagnostics: [
          {
            id: 'warning',
            kind: 'compiler',
            severity: 'warning',
            filePath: 'src/cart.ts',
            line: 1,
            column: 1,
            title: 'TS6133',
            message: 'Unused value',
            code: 'TS6133'
          }
        ],
        updatedAt: 1
      })
    ).toBe('No failures. 12 passed. 1 warning')
  })
})
