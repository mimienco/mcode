/**
 * Gap-plan smoke: safety rail persistence + personas IPC + mission hotkey.
 *
 * Why E2E and not unit tests:
 *  - `worktrees:updateMeta` personas ride a real main-process persistence
 *    round-trip (mcode-data.json write + worktrees snapshot fan-out).
 *  - `Mod+Shift+J` must prove a SECOND Electron window (dashboard popout)
 *    opens from a renderer keyboard event — multi-window lifecycle.
 *  - The automation create path crosses renderer -> IPC -> definition
 *    operation -> seeded-repo state, then back into the visible list.
 *
 * Notification toasts are intentionally NOT asserted here: headless Electron
 * notification delivery is platform-flaky and already covered by unit tests
 * around the shared dispatch handler gates.
 */

import { test, expect } from './helpers/mcode-app'
import { getStoreState } from './helpers/store'

test.describe('gap plan safety rail', () => {
  test('persists cost cap + push guard on a new automation and lists it', async ({
    mcodePage
  }) => {
    const created = await mcodePage.evaluate(async () => {
      const repos = await window.api.repos.list()
      const repo = repos[0]
      if (!repo) {
        throw new Error('seeded repo missing')
      }
      return await window.api.automations.create({
        name: 'E2E Cap Check',
        prompt: 'noop',
        agentId: 'claude',
        projectId: repo.id,
        workspaceMode: 'new_per_run',
        workspaceId: null,
        baseBranch: null,
        reuseSession: false,
        timezone: 'UTC',
        rrule: 'FREQ=DAILY;BYHOUR=3;BYMINUTE=0',
        dtstart: Date.now(),
        enabled: false,
        missedRunGraceMinutes: 5,
        maxCostUsdPerRun: 1.25,
        autoPushEnabled: false,
        executionTargetType: 'local',
        executionTargetId: '',
        schedulerOwner: 'local_host_service'
      } as never)
    })

    expect(created.id).toBeTruthy()
    expect(created.maxCostUsdPerRun).toBe(1.25)
    expect(created.autoPushEnabled).toBe(false)

    // User-observable: the automation shows up in the real list UI.
    await mcodePage.evaluate(() => {
      window.__store?.setState({ activeView: 'automations' })
    })
    await expect(mcodePage.getByText('E2E Cap Check', { exact: true }).first()).toBeVisible()

    const listed = await mcodePage.evaluate(async () => {
      const automations = await window.api.automations.list()
      return automations.find((entry) => entry.name === 'E2E Cap Check') ?? null
    })
    expect(listed?.maxCostUsdPerRun).toBe(1.25)
    expect(listed?.autoPushEnabled).toBe(false)

    await mcodePage.evaluate(async (id) => {
      await window.api.automations.delete({ id })
    }, created.id)
  })

  test('persists agent personas through updateMeta', async ({ mcodePage }) => {
    const worktreeId = await mcodePage.evaluate(async () => {
      const all = await window.api.worktrees.listAll()
      const first = Array.isArray(all) ? all[0] : undefined
      const id = (first as { id?: string } | undefined)?.id
      if (!id) {
        throw new Error('no worktree available from seeded repo')
      }
      await window.api.worktrees.updateMeta({
        worktreeId: id,
        updates: {
          personas: {
            codex: { name: 'Marshall', color: '#3f9142' }
          }
        }
      })
      return id
    })

    // Restart-grade proof would relaunch; here we prove the main-process
    // metadata read-back plus the worktree snapshot the renderer receives.
    const readBack = await mcodePage.evaluate(
      async ({ worktreeId }) => {
        const all = (await window.api.worktrees.listAll()) as Array<{
          id: string
          personas?: Record<string, { name?: string; color?: string }>
        }>
        return all.find((entry) => entry.id === worktreeId)?.personas ?? null
      },
      { worktreeId }
    )

    expect(readBack?.codex).toEqual({ name: 'Marshall', color: '#3f9142' })

    // Store-side projection the sidebar rows read from.
    // Renderer projection refreshes via the worktrees snapshot fan-out; poll.
    await expect
      .poll(
        async () => {
          const projected = await getStoreState<Record<string, unknown> | undefined>(
            mcodePage,
            'worktreesByRepo'
          )
          return JSON.stringify(projected ?? {})
        },
        { timeout: 10_000 }
      )
      .toContain('Marshall')
  })

  test('opens the dashboard popout via Mod+Shift+J', async ({ electronApp, mcodePage }) => {
    const windowsBefore = electronApp.windows().length

    // The popout is feature-gated; enable it for this profile first.
    await mcodePage.evaluate(() =>
      window.api.settings.set({ experimentalAgentDashboardPopout: true })
    )
    // A) Direct api call: proves popout creation works at all.
    await mcodePage.evaluate(() => {
      void window.api.dashboard.openPopout?.('mission')
    })
    await expect
      .poll(async () => mcodePage.evaluate(() => window.api.dashboard.getPopoutOpen()), {
        timeout: 15_000
      })
      .toBe(true)

    // B) Close the popout window through its own page.
    const popoutPage = electronApp.windows()[electronApp.windows().length - 1]
    await popoutPage.close()
    await expect
      .poll(async () => mcodePage.evaluate(() => window.api.dashboard.getPopoutOpen()), {
        timeout: 15_000
      })
      .toBe(false)

    // C) User-facing path: registered keyboard shortcut reopens it.
    await mcodePage.keyboard.press('Control+Shift+J')
    await expect
      .poll(async () => mcodePage.evaluate(() => window.api.dashboard.getPopoutOpen()), {
        timeout: 15_000
      })
      .toBe(true)
    await expect
      .poll(async () => electronApp.windows().length, { timeout: 15_000 })
      .toBeGreaterThan(windowsBefore)
  })
})
