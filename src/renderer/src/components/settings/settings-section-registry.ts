import type { ComponentType } from 'react'

// Why: PERF-03 stage B — one canonical lazy-loader map for every settings
// section so the Settings shell stays a small chunk while panes load on demand.

export type SettingsSectionEntry = {
  id: string
  load: () => Promise<{ default: ComponentType<unknown> }>
  /** Deduped warm-up of the loader promise (StrictMode-safe). */
  prefetch: () => void
  /** Why: test seam asserting laziness and prefetch dedupe without mocks. */
  peekLoadCount: () => number
}

/** Why: settings panes use named exports; normalize them for React.lazy. */
function entry<M extends Record<string, unknown>>(
  id: string,
  loadModule: () => Promise<M>,
  exportName: keyof M & string
): SettingsSectionEntry {
  let cachedPromise: Promise<{ default: ComponentType<unknown> }> | null = null
  let loadCallCount = 0
  const load = async (): Promise<{ default: ComponentType<unknown> }> => {
    const moduleExports = await loadModule()
    return { default: moduleExports[exportName] as unknown as ComponentType<unknown> }
  }
  return {
    id,
    load,
    prefetch: () => {
      if (!cachedPromise) {
        loadCallCount += 1
        cachedPromise = load().catch((error) => {
          // Why: allow a later prefetch/retry after a transient failure.
          cachedPromise = null
          throw error
        })
      }
    },
    peekLoadCount: () => loadCallCount
  }
}

const SECTION_ENTRIES: SettingsSectionEntry[] = [
  entry('general', () => import('./GeneralPane'), 'GeneralPane'),
  entry('setup-guide', () => import('./SettingsSetupGuidePane'), 'SettingsSetupGuidePane'),
  entry('agents', () => import('./AgentsPane'), 'AgentsPane'),
  entry('accounts', () => import('./AccountsPane'), 'AccountsPane'),
  entry('orchestration', () => import('./OrchestrationPane'), 'OrchestrationPane'),
  entry('linear', () => import('./LinearAgentSkillPane'), 'LinearAgentSkillPane'),
  entry('computer-use', () => import('./ComputerUsePane'), 'ComputerUsePane'),
  entry('voice', () => import('./VoicePane'), 'VoicePane'),
  entry('mcode-account', () => import('./MCodeAccountSettingsPane'), 'MCodeAccountSettingsPane'),
  entry('integrations', () => import('./IntegrationsPane'), 'IntegrationsPane'),
  entry('mobile', () => import('./MobileSettingsPane'), 'MobileSettingsPane'),
  entry('automations', () => import('./AutomationsSettingsPane'), 'AutomationsSettingsPane'),
  entry('artifacts', () => import('./ArtifactsSettingsPane'), 'ArtifactsSettingsPane'),
  entry('share-skills', () => import('./ShareSkillsSettingsPane'), 'ShareSkillsSettingsPane'),
  entry('git', () => import('./GitPane'), 'GitPane'),
  entry('tasks', () => import('./TasksPane'), 'TasksPane'),
  entry('terminal', () => import('./TerminalPane'), 'TerminalPane'),
  entry('quick-commands', () => import('./QuickCommandsPane'), 'QuickCommandsPane'),
  entry('browser', () => import('./BrowserPane'), 'BrowserPane'),
  entry(
    'mobile-emulator',
    () => import('./MobileEmulatorSettingsPane'),
    'MobileEmulatorSettingsPane'
  ),
  entry('floating-workspace', () => import('./FloatingWorkspacePane'), 'FloatingWorkspacePane'),
  entry('appearance', () => import('./AppearancePane'), 'AppearancePane'),
  entry('input', () => import('./InputPane'), 'InputPane'),
  entry('notifications', () => import('./NotificationsPane'), 'NotificationsPane'),
  entry('shortcuts', () => import('./ShortcutsPane'), 'ShortcutsPane'),
  entry('stats', () => import('../stats/StatsPane'), 'StatsPane'),
  entry('ssh', () => import('./SshPane'), 'SshPane'),
  entry('servers', () => import('./RuntimeEnvironmentsPane'), 'RuntimeEnvironmentsPane'),
  entry(
    'developer-permissions',
    () => import('./DeveloperPermissionsPane'),
    'DeveloperPermissionsPane'
  ),
  entry('privacy', () => import('./PrivacyPane'), 'PrivacyPane'),
  entry('advanced', () => import('./AdvancedPane'), 'AdvancedPane'),
  entry('experimental', () => import('./ExperimentalPane'), 'ExperimentalPane'),
  entry('plugins', () => import('./PluginsSettingsSection'), 'PluginsSettingsSection'),
  entry('repository', () => import('./RepositoryPane'), 'RepositoryPane'),
  // Why: dev tools stay DEV-gated exactly as before the registry existed.
  ...(import.meta.env.DEV ? [entry('dev', () => import('./DevToolsPane'), 'DevToolsPane')] : [])
]

const ENTRIES_BY_ID = new Map(
  SECTION_ENTRIES.map((sectionEntry) => [sectionEntry.id, sectionEntry])
)

export function getSettingsSectionRegistry(): ReadonlyMap<string, SettingsSectionEntry> {
  return ENTRIES_BY_ID
}

/** Exact id first, then repo-* deep links fall back to the repository pane. */
export function findSectionEntry(sectionId: string): SettingsSectionEntry | null {
  return (
    ENTRIES_BY_ID.get(sectionId) ??
    (sectionId.startsWith('repo-') ? (ENTRIES_BY_ID.get('repository') ?? null) : null)
  )
}

let idlePrefetchScheduled = false

/**
 * Why: warm the default section's chunk after app idle so opening Settings on
 * the default tab never waits for the network/disk round-trip.
 */
export function prefetchDefaultSections(): void {
  if (idlePrefetchScheduled) {
    return
  }
  idlePrefetchScheduled = true
  const run = (): void => {
    idlePrefetchScheduled = false
    findSectionEntry('general')?.prefetch()
  }
  if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(run)
  } else {
    setTimeout(run, 200)
  }
}
