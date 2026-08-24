import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

function readSource(relativePath: string): string {
  return readFileSync(join(process.cwd(), relativePath), 'utf8').replace(/\r\n/g, '\n')
}

const WORKSPACE_SHELL_PATH = 'src/renderer/src/app-shell/AppWorkspaceShell.tsx'

// PERF-03 stage A: cold navigation must never swap the center surface to a
// null Suspense fallback; a named loading surface plus deferred view commit
// keeps the last stable page visible while chunks load.
describe('workspace shell loading surface', () => {
  it('never mounts a null Suspense fallback in the workspace shell', () => {
    const source = readSource(WORKSPACE_SHELL_PATH)
    expect(source).not.toContain('fallback={null}')
  })

  it('uses the static PageLoadingSurface as the non-null Suspense fallback', () => {
    const source = readSource(WORKSPACE_SHELL_PATH)
    expect(source).toContain('PageLoadingSurface')
    expect((source.match(/<PageLoadingSurface \/>/g) ?? []).length).toBeGreaterThanOrEqual(2)
  })

  it('commits page navigation through the stable-view seam so the prior page stays mounted', () => {
    const source = readSource(WORKSPACE_SHELL_PATH)
    expect(source).toContain('useStableActiveView')
    expect(source).toMatch(/displayedView/)
  })
})
