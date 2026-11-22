import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

function source(relativePath: string): string {
  return readFileSync(join(__dirname, relativePath), 'utf8')
}

describe('conditionally suppressed tooltips', () => {
  it('keeps the browser reload tooltip controlled while its menu opens', () => {
    const toolbar = source('browser-pane/assemble-chrome/browser-page-toolbar.tsx')
    expect(toolbar).toContain('open={reloadTooltipOpen && !reloadMenuOpen}')
    expect(toolbar).toContain(
      'onOpenChange={(open) => setReloadTooltipOpen(open && !reloadMenuOpen)}'
    )
    expect(toolbar).not.toContain('open={reloadMenuOpen ? false : undefined}')
  })

  it('keeps the workspace-board tooltip controlled before and after its one-time hint', () => {
    const toolbar = source('sidebar/SidebarToolbar.tsx')
    expect(toolbar).toContain('open={workspaceBoardMovedHintOpen || workspaceBoardTooltipOpen}')
    expect(toolbar).toContain('onOpenChange={setWorkspaceBoardTooltipOpen}')
    expect(toolbar).not.toContain('open={workspaceBoardMovedHintOpen ? true : undefined}')
  })
})
