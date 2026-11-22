import { expect, test } from './helpers/mcode-app'

const MAX_COLD_SETTINGS_OPEN_MS = 500
const MAX_WARM_SETTINGS_OPEN_MS = 200
const MAX_PRIMARY_PAGE_OPEN_MS = 750
const MAX_NAVIGATION_LONG_TASK_MS = 250

test.use({ seedTestRepo: false })

test('settings opens at native-feeling latency on cold and warm navigation', async ({
  mcodePage
}, testInfo) => {
  const settingsSearch = mcodePage.getByPlaceholder('Search settings')

  const measureSettingsOpen = async (): Promise<{ openMs: number; longTasks: number[] }> =>
    mcodePage.evaluate(
      () =>
        new Promise((resolve, reject) => {
          const settingsButton = document.querySelector<HTMLButtonElement>(
            'button[aria-label="Settings"]'
          )
          if (!settingsButton) {
            reject(new Error('Settings button is unavailable'))
            return
          }

          const longTasks: number[] = []
          const longTaskObserver = new PerformanceObserver((list) => {
            for (const entry of list.getEntries()) {
              longTasks.push(Math.round(entry.duration))
            }
          })
          longTaskObserver.observe({ type: 'longtask', buffered: false })

          const startedAt = performance.now()
          const mutationObserver = new MutationObserver(() => {
            if (!document.querySelector('input[placeholder="Search settings"]')) {
              return
            }
            mutationObserver.disconnect()
            longTaskObserver.disconnect()
            // Why: hidden Electron windows throttle rAF to roughly 1 Hz, so the
            // stable cross-environment seam is click-to-visible DOM commit.
            resolve({ openMs: Math.round(performance.now() - startedAt), longTasks })
          })
          mutationObserver.observe(document.body, { childList: true, subtree: true })
          settingsButton.click()
        })
    )

  const coldMeasurement = await measureSettingsOpen()
  await settingsSearch.waitFor({ state: 'visible', timeout: 10_000 })

  await mcodePage.getByRole('button', { name: 'Back to app' }).click()
  await settingsSearch.waitFor({ state: 'hidden' })

  const warmMeasurement = await measureSettingsOpen()
  await settingsSearch.waitFor({ state: 'visible', timeout: 10_000 })

  await testInfo.attach('settings-native-feel', {
    body: await mcodePage.screenshot({ animations: 'disabled' }),
    contentType: 'image/png'
  })

  await testInfo.attach('app-navigation-latency', {
    body: JSON.stringify({ cold: coldMeasurement, warm: warmMeasurement }, null, 2),
    contentType: 'application/json'
  })
  console.log(
    `[app-navigation-latency] cold=${JSON.stringify(coldMeasurement)} warm=${JSON.stringify(warmMeasurement)}`
  )

  expect(
    coldMeasurement.openMs,
    `cold Settings navigation took ${coldMeasurement.openMs}ms`
  ).toBeLessThan(MAX_COLD_SETTINGS_OPEN_MS)
  expect(
    warmMeasurement.openMs,
    `warm Settings navigation took ${warmMeasurement.openMs}ms`
  ).toBeLessThan(MAX_WARM_SETTINGS_OPEN_MS)
  expect(
    Math.max(0, ...coldMeasurement.longTasks, ...warmMeasurement.longTasks),
    'Settings navigation produced a main-thread stall'
  ).toBeLessThan(MAX_NAVIGATION_LONG_TASK_MS)
})

type PrimaryPageProbe = {
  route: 'tasks' | 'automations' | 'artifacts' | 'skills' | 'mobile'
  selector: string
  text?: string
}

const PRIMARY_PAGE_PROBES: readonly PrimaryPageProbe[] = [
  { route: 'tasks', selector: 'button[aria-label="Close tasks"]' },
  { route: 'automations', selector: 'main h1', text: 'Automations' },
  { route: 'artifacts', selector: 'main h1', text: 'Artifacts' },
  { route: 'skills', selector: '[data-skills-page-list="true"]' },
  { route: 'mobile', selector: '.mobile-page-root' }
]

test('primary app pages commit without slow warm-render fan-out', async ({
  mcodePage
}, testInfo) => {
  const measurements: { route: string; openMs: number; longTasks: number[] }[] = []

  for (const probe of PRIMARY_PAGE_PROBES) {
    const measurement = await mcodePage.evaluate(
      ({ route, selector, text }) =>
        new Promise<{ openMs: number; longTasks: number[] }>((resolve, reject) => {
          const store = window.__store
          if (!store) {
            reject(new Error('window.__store is unavailable'))
            return
          }

          const targetIsPresent = (): boolean =>
            Array.from(document.querySelectorAll(selector)).some(
              (element) => text === undefined || element.textContent?.trim() === text
            )
          const longTasks: number[] = []
          const longTaskObserver = new PerformanceObserver((list) => {
            for (const entry of list.getEntries()) {
              longTasks.push(Math.round(entry.duration))
            }
          })
          longTaskObserver.observe({ type: 'longtask', buffered: false })

          const startedAt = performance.now()
          const timeout = window.setTimeout(() => {
            mutationObserver.disconnect()
            longTaskObserver.disconnect()
            reject(new Error(`${route} did not commit within 10 seconds`))
          }, 10_000)
          const finish = (): void => {
            if (!targetIsPresent()) {
              return
            }
            window.clearTimeout(timeout)
            mutationObserver.disconnect()
            longTaskObserver.disconnect()
            resolve({ openMs: Math.round(performance.now() - startedAt), longTasks })
          }
          const mutationObserver = new MutationObserver(finish)
          mutationObserver.observe(document.body, { childList: true, subtree: true })

          const state = store.getState()
          switch (route) {
            case 'tasks':
              state.openTaskPage()
              break
            case 'automations':
              state.openAutomationsPage()
              break
            case 'artifacts':
              state.openArtifactsPage()
              break
            case 'skills':
              state.openSkillsPage()
              break
            case 'mobile':
              state.openMobilePage()
              break
          }
          finish()
        }),
      probe
    )

    measurements.push({ route: probe.route, ...measurement })
    await expect(mcodePage.locator(probe.selector).filter({ hasText: probe.text })).toBeVisible()
    if (probe.route === 'automations') {
      await testInfo.attach('automations-native-feel', {
        body: await mcodePage.screenshot({ animations: 'disabled' }),
        contentType: 'image/png'
      })
    }
  }

  console.log(`[app-navigation-latency] pages=${JSON.stringify(measurements)}`)
  await testInfo.attach('primary-app-page-latency', {
    body: JSON.stringify(measurements, null, 2),
    contentType: 'application/json'
  })

  for (const measurement of measurements) {
    expect(
      measurement.openMs,
      `${measurement.route} navigation took ${measurement.openMs}ms`
    ).toBeLessThan(MAX_PRIMARY_PAGE_OPEN_MS)
    expect(
      Math.max(0, ...measurement.longTasks),
      `${measurement.route} navigation produced a main-thread stall`
    ).toBeLessThan(MAX_NAVIGATION_LONG_TASK_MS)
  }
})
