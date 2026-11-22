import { expect, test } from './helpers/mcode-app'
import { waitForSessionReady } from './helpers/store'

// Why: right-sidebar panels are React.lazy chunks. A regression that reintroduces
// click-time chunk waterfalls (or a chunk that throws at module evaluation — see
// the node:fs externalization crash) shows up here as a multi-second switch or a
// stuck error boundary. Idle warming (prefetch-shell-chunks) plus hover
// prefetching should keep every switch well under these budgets; the assertions
// exist to fail loudly when a panel stops landing on an already-loaded module.
//
// Why store-driven switching instead of clicking the activity bar: the bar
// overflows into a "More tabs" menu at narrow window widths, so which tabs have
// visible buttons is layout-dependent. This deliberately exercises the
// keyboard/no-hover worst case; pointer and focus intent warm the same chunks
// earlier and should normally settle faster.

const ERROR_BOUNDARY_TITLE = 'The right sidebar hit an error.'

type TabProbe = {
  id: string
  /** Selector for the panel's own root content (not a Suspense fallback). */
  contentSelector: string
}
const PROBES: TabProbe[] = [
  { id: 'vault', contentSelector: "[data-right-sidebar-root] [class*='ai-vault']" },
  { id: 'explorer', contentSelector: '[data-right-sidebar-root] [data-mcode-explorer-shell]' }
]

const MAX_VISIBLE_FEEDBACK_MS = 200
const MAX_COLD_SETTLE_MS = 1_750
const SWITCH_SETTLE_TIMEOUT_MS = 2_500

test('right-sidebar fills the shell and opens tabs without hitting the error boundary', async ({
  mcodePage
}, testInfo) => {
  await waitForSessionReady(mcodePage)

  // Why: a persisted zero width renders every panel hidden even when "open",
  // which is layout state, not the chunk latency this spec measures.
  await mcodePage.evaluate(() => {
    const store = window.__store
    if (!store) {
      throw new Error('window.__store is unavailable')
    }
    if (!store.getState().rightSidebarOpen) {
      store.getState().toggleRightSidebar()
    }
    if (store.getState().rightSidebarWidth < 200) {
      store.getState().setRightSidebarWidth(320)
    }
  })

  const sidebarRoot = mcodePage.locator('[data-right-sidebar-root]')
  const sidebarPanel = sidebarRoot.locator(':scope > div').first()
  const statusBar = mcodePage
    .getByRole('button', { name: 'Refresh rate limits' })
    .locator('xpath=ancestor::div[contains(@class, "min-h-[24px]")][1]')
  const [rootBox, panelBox, statusBarBox, viewportHeight] = await Promise.all([
    sidebarRoot.boundingBox(),
    sidebarPanel.boundingBox(),
    statusBar.boundingBox(),
    mcodePage.evaluate(() => window.innerHeight)
  ])
  expect(rootBox, 'right-sidebar root should be rendered').not.toBeNull()
  expect(panelBox, 'right-sidebar panel should be rendered').not.toBeNull()
  expect(statusBarBox, 'status bar should be rendered').not.toBeNull()
  expect(
    Math.abs((rootBox?.height ?? 0) + (statusBarBox?.height ?? 0) - viewportHeight),
    'right-sidebar root plus status bar should span the Electron content height'
  ).toBeLessThanOrEqual(1)
  expect(
    Math.abs((rootBox?.y ?? 0) + (rootBox?.height ?? 0) - (statusBarBox?.y ?? 0)),
    'right-sidebar root should end exactly where the status bar begins'
  ).toBeLessThanOrEqual(1)
  expect(
    Math.abs((rootBox?.height ?? 0) - (panelBox?.height ?? 0)),
    'right-sidebar panel should fill the shell height'
  ).toBeLessThanOrEqual(1)

  const measurements: { tab: string; feedbackMs: number; settleMs: number }[] = []

  for (const probe of PROBES) {
    const startedAt = Date.now()
    await mcodePage.evaluate((tab) => {
      const store = window.__store
      if (!store) {
        throw new Error('window.__store is unavailable')
      }
      store.getState().setRightSidebarTab(tab)
    }, probe.id)
    const content = mcodePage.locator(probe.contentSelector).first()
    const visibleFeedback = content.or(sidebarRoot.getByRole('status')).first()
    await visibleFeedback.waitFor({
      state: 'visible',
      timeout: MAX_VISIBLE_FEEDBACK_MS
    })
    const feedbackMs = Date.now() - startedAt
    await content.waitFor({
      state: 'visible',
      timeout: SWITCH_SETTLE_TIMEOUT_MS
    })
    const settleMs = Date.now() - startedAt
    measurements.push({ tab: probe.id, feedbackMs, settleMs })

    expect(mcodePage.getByText(ERROR_BOUNDARY_TITLE)).toHaveCount(0)
  }

  await testInfo.attach('right-sidebar-open-latency', {
    body: JSON.stringify(measurements, null, 2),
    contentType: 'application/json'
  })
  console.log(`[right-sidebar-open-latency] ${JSON.stringify(measurements)}`)

  await testInfo.attach('right-sidebar-native-feel', {
    body: await mcodePage.screenshot({ animations: 'disabled' }),
    contentType: 'image/png'
  })

  for (const { tab, feedbackMs, settleMs } of measurements) {
    expect(feedbackMs, `${tab} panel took ${feedbackMs}ms to show feedback`).toBeLessThan(
      MAX_VISIBLE_FEEDBACK_MS
    )
    expect(settleMs, `${tab} panel took ${settleMs}ms to settle cold`).toBeLessThan(
      MAX_COLD_SETTLE_MS
    )
  }
})
