import { expect, test } from './helpers/mcode-app'
import { ensureTerminalVisible, waitForActiveWorktree, waitForSessionReady } from './helpers/store'

test.describe('Pane actions alignment', () => {
  test.beforeEach(async ({ mcodePage }) => {
    await waitForSessionReady(mcodePage)
    await waitForActiveWorktree(mcodePage)
    await ensureTerminalVisible(mcodePage)
  })

  test('rightmost pane actions reach the pane edge when the right sidebar is closed', async ({
    mcodePage
  }) => {
    const rightGroupId = await mcodePage.evaluate(() => {
      const store = window.__store
      if (!store) {
        throw new Error('window.__store is not available')
      }
      const state = store.getState()
      const worktreeId = state.activeWorktreeId
      if (!worktreeId) {
        throw new Error('No active worktree for split-group setup')
      }
      const sourceGroupId = state.ensureWorktreeRootGroup(worktreeId)
      const groupId = state.createEmptySplitGroup(worktreeId, sourceGroupId, 'right')
      if (!groupId) {
        throw new Error('Failed to create the right split group')
      }

      const tab = state.createTab(worktreeId, groupId, undefined, { activate: true })
      state.focusGroup(worktreeId, groupId)
      state.setActiveTab(tab.id)
      state.setActiveTabType('terminal')
      state.setRightSidebarOpen(false)
      return groupId
    })

    const strip = mcodePage.locator(`[data-tab-group-strip-id="${rightGroupId}"]`)
    const paneActions = strip.getByRole('button', { name: 'Pane Actions' })
    await expect(paneActions).toBeVisible()

    const geometry = await strip.evaluate((element) => {
      const button = element.querySelector<HTMLButtonElement>('button[aria-label="Pane Actions"]')
      if (!button) {
        throw new Error('Pane Actions button is missing from the rightmost split group')
      }
      const stripRect = element.getBoundingClientRect()
      const buttonRect = button.getBoundingClientRect()
      return {
        trailingGap: stripRect.right - buttonRect.right,
        stripTop: stripRect.top,
        windowControlsBottom:
          document.querySelector<HTMLElement>('.window-controls')?.getBoundingClientRect().bottom ?? 0
      }
    })

    expect(geometry.stripTop).toBeGreaterThanOrEqual(geometry.windowControlsBottom)
    expect(geometry.trailingGap).toBeLessThanOrEqual(12)
  })
})
