import type { ActiveRightSidebarTab } from '@/store/slices/editor'

// Why: hovering an activity-bar icon is the earliest signal of the click —
// warming that panel's chunk on hover (instead of waiting for the click) hides
// the chunk-fetch + evaluate waterfall behind the ~100ms of pointer travel, so
// first opens feel native. Idle warming (app-shell/prefetch-shell-chunks)
// covers the no-hover case; this covers the impatient case.

const PANEL_CHUNK_IMPORTERS: Partial<Record<ActiveRightSidebarTab, () => Promise<unknown>>> = {
  explorer: () => import('./FileExplorer'),
  'source-control': () => import('./SourceControl'),
  checks: () => import('./ChecksPanel'),
  problems: () => import('./WorkspaceProblemsPanel'),
  ports: () => import('./PortsPanel'),
  vault: () => import('./AiVaultPanel'),
  workspaces: () => import('./FolderWorkspaceWorktreesPanel'),
  'pr-checks': () => import('./FolderWorkspacePrChecksPanel')
}

const warmed = new Set<ActiveRightSidebarTab>()
let pendingHoverTab: ActiveRightSidebarTab | null = null
let pendingHoverTimer: ReturnType<typeof setTimeout> | null = null

function clearPendingHover(): void {
  if (pendingHoverTimer !== null) {
    clearTimeout(pendingHoverTimer)
    pendingHoverTimer = null
  }
  pendingHoverTab = null
}

export function prefetchRightSidebarPanel(tab: ActiveRightSidebarTab): void {
  if (pendingHoverTab === tab) {
    clearPendingHover()
  }
  if (warmed.has(tab)) {
    return
  }
  const importer = PANEL_CHUNK_IMPORTERS[tab]
  if (!importer) {
    return
  }
  warmed.add(tab)
  importer().catch(() => {
    // Why: lazyWithRetry already owns retry/backoff on the real open path; a
    // failed hover-warm must not surface as an unhandled rejection.
    warmed.delete(tab)
  })
}

export function scheduleRightSidebarPanelPrefetch(tab: ActiveRightSidebarTab): void {
  if (warmed.has(tab) || pendingHoverTab === tab) {
    return
  }
  clearPendingHover()
  pendingHoverTab = tab
  pendingHoverTimer = setTimeout(() => {
    pendingHoverTimer = null
    pendingHoverTab = null
    prefetchRightSidebarPanel(tab)
  }, 60)
}

export function cancelRightSidebarPanelPrefetch(tab: ActiveRightSidebarTab): void {
  if (pendingHoverTab === tab) {
    clearPendingHover()
  }
}
