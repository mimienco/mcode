import { createDesktopInvoke } from './desktop-client'

// ─── Types (bootstrap-required surface of src/preload/api/app-api.ts) ───────
export type AppIdentity = {
  name: string
  isDev: boolean
  devLabel: string | null
  devBranch: string | null
  devWorktreeName: string | null
  devRepoRoot: string | null
  dockBadgeLabel: string | null
}

type AppInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

/**
 * The `window.api.app` surface under Tauri. Methods with a native Rust
 * command invoke it; the rest carry the web adapter's honest defaults
 * (web-preload-api.ts:561-613) because their main-process side is not ported
 * yet — present-but-inert beats undefined (which crashes bootstrap) and beats
 * fake success claims. The health-probe pair is deliberately inert: the
 * watchdog main-process emitter has no Tauri port, so the renderer reports
 * nothing and the probe never fires.
 */
export function createTauriAppClient(invoke: AppInvoke): Record<string, unknown> {
  const call = createDesktopInvoke(invoke)
  return {
    reportRendererHealth: () => undefined,
    onRendererHealthProbe: () => () => undefined,
    getIdentity: async () => {
      try {
        const context = (await call('app_get_launch_context')) as { version?: unknown }
        return {
          name: 'MCode',
          isDev: typeof context?.version === 'string' && context.version.includes('dev'),
          devLabel: null,
          devBranch: null,
          devWorktreeName: null,
          devRepoRoot: null,
          dockBadgeLabel: null
        }
      } catch {
        return {
          name: 'MCode',
          isDev: false,
          devLabel: null,
          devBranch: null,
          devWorktreeName: null,
          devRepoRoot: null,
          dockBadgeLabel: null
        }
      }
    },
    getFeatureWallAssetBaseUrl: () => Promise.resolve('/'),
    relaunch: () => call('app_request_shutdown') as Promise<void>,
    restart: () => call('app_request_shutdown') as Promise<void>,
    reload: () => Promise.resolve(window.location.reload()),
    stageBeforeUnloadSync: () => undefined,
    awaitBeforeUnloadCheckpoint: () => Promise.resolve(),
    awaitFirstWindowStartupServices: () => Promise.resolve(),
    recoverLegacyWorkerTerminalsForRendererStartup: () => Promise.resolve(),
    startupDiagnostic: () => Promise.resolve(),
    getKeyboardInputSourceId: () => Promise.resolve(null),
    getMacCapturedDigitRowChords: () => Promise.resolve([]),
    getKeyboardLayoutSnapshot: () => Promise.resolve(null),
    onKeyboardLayoutChanged: () => () => undefined,
    setUnreadDockBadgeCount: () => Promise.resolve(),
    getFloatingTerminalCwd: () => Promise.resolve(''),
    getFloatingMarkdownDirectory: () => Promise.resolve(''),
    pickFloatingMarkdownDocument: () => Promise.resolve(null),
    pickFloatingWorkspaceDirectory: () => Promise.resolve(null),
    writeTerminalRenderDesyncEvidence: () =>
      Promise.reject(new Error('Terminal render evidence is not available under Tauri yet.'))
  }
}
