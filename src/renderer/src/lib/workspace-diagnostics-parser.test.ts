import { describe, expect, it } from 'vitest'

import { parseWorkspaceDiagnosticsOutput } from './workspace-diagnostics-parser'

describe('parseWorkspaceDiagnosticsOutput', () => {
  it('turns Vitest failures and their source frames into clickable test diagnostics', () => {
    const run = parseWorkspaceDiagnosticsOutput(
      [
        ' FAIL  src/cart/cart.test.ts > cart totals > applies discounts',
        'AssertionError: expected 12 to be 10',
        ' ❯ src/cart/cart.test.ts:142:7',
        ' Test Files  1 failed | 4 passed (5)',
        '      Tests  1 failed | 19 passed (20)'
      ].join('\n'),
      1
    )

    expect(run).toMatchObject({
      tool: 'vitest',
      status: 'failed',
      failed: 1,
      passed: 19,
      total: 20
    })
    expect(run?.diagnostics).toEqual([
      expect.objectContaining({
        kind: 'test',
        severity: 'error',
        filePath: 'src/cart/cart.test.ts',
        line: 142,
        column: 7,
        title: 'cart totals > applies discounts',
        message: 'expected 12 to be 10'
      })
    ])
  })

  it('parses Jest failure blocks and keeps the assertion location', () => {
    const run = parseWorkspaceDiagnosticsOutput(
      [
        'FAIL src/user/user.test.ts',
        '  user service',
        '    ✕ rejects duplicate email (18 ms)',
        '',
        '  ● user service › rejects duplicate email',
        '',
        '    expect(received).toBe(expected)',
        '',
        '      at Object.<anonymous> (src/user/user.test.ts:81:13)',
        '',
        'Tests:       1 failed, 8 passed, 9 total'
      ].join('\n'),
      1
    )

    expect(run).toMatchObject({ tool: 'jest', status: 'failed', failed: 1, passed: 8, total: 9 })
    expect(run?.diagnostics[0]).toMatchObject({
      filePath: 'src/user/user.test.ts',
      line: 81,
      column: 13,
      title: 'user service › rejects duplicate email',
      message: 'expect(received).toBe(expected)'
    })
  })

  it('parses pytest short summaries into one failure per test', () => {
    const run = parseWorkspaceDiagnosticsOutput(
      [
        '=================== FAILURES ===================',
        '________________ test_rejects_expired_token ________________',
        'tests/test_auth.py:64: AssertionError',
        '================ short test summary info ================',
        'FAILED tests/test_auth.py::test_rejects_expired_token - AssertionError: token accepted',
        '================ 1 failed, 11 passed in 0.42s ================'
      ].join('\n'),
      1
    )

    expect(run).toMatchObject({ tool: 'pytest', status: 'failed', failed: 1, passed: 11, total: 12 })
    expect(run?.diagnostics).toEqual([
      expect.objectContaining({
        filePath: 'tests/test_auth.py',
        line: 64,
        column: null,
        title: 'test_rejects_expired_token',
        message: 'AssertionError: token accepted'
      })
    ])
  })

  it('collects TypeScript compiler errors even when the command has no summary', () => {
    const run = parseWorkspaceDiagnosticsOutput(
      [
        "src/domain/order.ts(27,9): error TS2322: Type 'string' is not assignable to type 'number'.",
        "src/domain/order.ts(44,3): warning TS6133: 'draft' is declared but its value is never read."
      ].join('\n'),
      2
    )

    expect(run).toMatchObject({ tool: 'typescript', status: 'failed', failed: 1, total: 2 })
    expect(run?.diagnostics).toEqual([
      expect.objectContaining({ line: 27, column: 9, severity: 'error', code: 'TS2322' }),
      expect.objectContaining({ line: 44, column: 3, severity: 'warning', code: 'TS6133' })
    ])
  })

  it('normalizes ESLint and Ruff locations into lint diagnostics', () => {
    const eslint = parseWorkspaceDiagnosticsOutput(
      [
        'src/ui/price.tsx',
        '  18:5  error  Unexpected any. Specify a different type  @typescript-eslint/no-explicit-any',
        '  26:3  warning  React Hook is missing a dependency  react-hooks/exhaustive-deps',
        '',
        '✖ 2 problems (1 error, 1 warning)'
      ].join('\n'),
      1
    )
    const ruff = parseWorkspaceDiagnosticsOutput(
      'src/api/session.py:12:5: F401 [*] `typing.Any` imported but unused',
      1
    )

    expect(eslint).toMatchObject({ tool: 'eslint', failed: 1, total: 2 })
    expect(eslint?.diagnostics[0]).toMatchObject({
      filePath: 'src/ui/price.tsx',
      line: 18,
      column: 5,
      code: '@typescript-eslint/no-explicit-any'
    })
    expect(ruff).toMatchObject({ tool: 'ruff', failed: 1, total: 1 })
    expect(ruff?.diagnostics[0]).toMatchObject({
      filePath: 'src/api/session.py',
      line: 12,
      column: 5,
      code: 'F401'
    })
  })

  it('recognizes successful test summaries so a rerun can clear stale failures', () => {
    expect(parseWorkspaceDiagnosticsOutput('Tests  20 passed (20)', 0)).toMatchObject({
      tool: 'vitest',
      status: 'passed',
      failed: 0,
      passed: 20,
      total: 20
    })
    expect(
      parseWorkspaceDiagnosticsOutput('================ 12 passed in 0.18s ================', 0)
    ).toMatchObject({ tool: 'pytest', status: 'passed', failed: 0, passed: 12, total: 12 })
  })
})
