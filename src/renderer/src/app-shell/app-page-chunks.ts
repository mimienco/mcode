export const APP_PAGE_IMPORTERS = {
  activity: () => import('../components/activity/ActivityPrototypePage'),
  artifacts: () => import('../components/artifacts/ArtifactsPage'),
  automations: () => import('../components/automations/AutomationsPage'),
  mobile: () => import('../components/mobile/MobilePage'),
  settings: () => import('../components/settings/Settings'),
  skills: () => import('../components/skills/SkillsPage'),
  space: () => import('../components/workspace-space/WorkspaceSpacePage'),
  tasks: () => import('../components/TaskPage')
} as const

export type PrefetchableAppPage = keyof typeof APP_PAGE_IMPORTERS
type AppPageImporterMap = Record<PrefetchableAppPage, () => Promise<unknown>>

export function createAppPagePrefetcher(
  importers: AppPageImporterMap
): (page: PrefetchableAppPage) => Promise<unknown> {
  const requests = new Map<PrefetchableAppPage, Promise<unknown>>()

  return (page) => {
    const existing = requests.get(page)
    if (existing) {
      return existing
    }

    const request = importers[page]().catch((error: unknown) => {
      // Why: a transient fetch failure must not poison pointer intent forever;
      // lazyWithRetry still owns visible-route recovery when navigation commits.
      requests.delete(page)
      throw error
    })
    requests.set(page, request)
    return request
  }
}

export const prefetchAppPage = createAppPagePrefetcher(APP_PAGE_IMPORTERS)

let pendingHoverPage: PrefetchableAppPage | null = null
let pendingHoverTimer: ReturnType<typeof setTimeout> | null = null

function clearPendingHover(): void {
  if (pendingHoverTimer !== null) {
    clearTimeout(pendingHoverTimer)
    pendingHoverTimer = null
  }
  pendingHoverPage = null
}

export function prefetchAppPageNow(page: PrefetchableAppPage): void {
  if (pendingHoverPage === page) {
    clearPendingHover()
  }
  void prefetchAppPage(page).catch(() => {})
}

export function scheduleAppPagePrefetch(page: PrefetchableAppPage): void {
  if (pendingHoverPage === page) {
    return
  }
  clearPendingHover()
  pendingHoverPage = page
  pendingHoverTimer = setTimeout(() => {
    pendingHoverTimer = null
    pendingHoverPage = null
    void prefetchAppPage(page).catch(() => {})
  }, 60)
}

export function cancelAppPagePrefetch(page: PrefetchableAppPage): void {
  if (pendingHoverPage === page) {
    clearPendingHover()
  }
}
