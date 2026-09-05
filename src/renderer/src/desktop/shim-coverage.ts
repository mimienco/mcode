/**
 * Coverage ledger for the Tauri `window.api` shim migration: every
 * `window.api.<namespace>` the renderer touches (extracted with
 * `grep -rEoh "window\.api\.[a-zA-Z_][a-zA-Z0-9_]*" src/renderer
 * --include=*.ts --include=*.tsx | cut -d. -f3 | sort -u`, exactly 77
 * including `e2e`) is classified as already bridged or still pending.
 *
 * `SHIMMED_NAMESPACES` lists what the shim layer serves natively under
 * Tauri today: `ui` (clipboard subset + the app-menu/zoom/toggle/check-
 * updates event bridge + popupMenu), `notifications`, `speech`, and the
 * full `fs` (34 methods), `shell` (12 methods), and `updater`
 * (11 methods) namespaces.
 */
export const ALL_WINDOW_API_NAMESPACES: readonly string[] = Object.freeze([
  'agentAwake',
  'agentStatus',
  'agentTrust',
  'aiVault',
  'app',
  'automations',
  'bitbucket',
  'browser',
  'cache',
  'claudeAccounts',
  'claudeUsage',
  'cli',
  'codexAccounts',
  'codexConfigSync',
  'codexUsage',
  'computerUsePermissions',
  'crashReports',
  'dashboard',
  'developerPermissions',
  'diagnostics',
  'e2e',
  'emulator',
  'ephemeralVm',
  'export',
  'feedback',
  'folderWorkspaces',
  'fs',
  'gh',
  'git',
  'gitBash',
  'gl',
  'grokAccounts',
  'hooks',
  'hostedReview',
  'jira',
  'keybindings',
  'linear',
  'localhostWorktreeLabels',
  'mcodeProfiles',
  'memory',
  'minimaxCredentials',
  'mobile',
  'nativeChat',
  'notebook',
  'notifications',
  'onboarding',
  'openCodeUsage',
  'pet',
  'platform',
  'plugins',
  'preflight',
  'projectGroups',
  'projects',
  'pty',
  'pwsh',
  'rateLimits',
  'remoteWorkspace',
  'repos',
  'runtime',
  'runtimeEnvironments',
  'session',
  'settings',
  'shell',
  'skills',
  'sparsePresets',
  'speech',
  'ssh',
  'starNag',
  'stats',
  'terminalPreview',
  'ui',
  'updater',
  'workspaceCleanup',
  'workspacePorts',
  'workspaceSpace',
  'worktrees',
  'wsl'
])

export const SHIMMED_NAMESPACES: readonly string[] = Object.freeze([
  'fs',
  'notifications',
  'shell',
  'speech',
  'ui',
  'updater'
])

export type ShimCoverage = {
  covered: readonly string[]
  pending: readonly string[]
  unexpected: readonly string[]
}

function isSorted(namespaces: readonly string[]): boolean {
  for (let i = 1; i < namespaces.length; i++) {
    if (namespaces[i - 1]! > namespaces[i]) {
      return false
    }
  }
  return true
}

function intersect(all: readonly string[], shimmed: readonly string[]): string[] {
  return all.filter((namespace) => shimmed.includes(namespace))
}

/**
 * Partitions the renderer's `window.api` namespaces into shimmed and pending
 * sets. `unexpected` carries names present in one list but not the full
 * extraction (stale ledger entries); tests fail on a non-empty result.
 */
export function computeShimCoverage(
  all: readonly string[] = ALL_WINDOW_API_NAMESPACES,
  shimmed: readonly string[] = SHIMMED_NAMESPACES
): ShimCoverage {
  if (!isSorted(all) || !isSorted(shimmed)) {
    throw new Error('namespace lists must be sorted')
  }
  return {
    covered: intersect(all, shimmed),
    pending: all.filter((namespace) => !shimmed.includes(namespace)),
    // Stale ledger entry: a shimmed namespace the renderer no longer touches.
    unexpected: shimmed.filter((namespace) => !all.includes(namespace))
  }
}
