import type {
  LinuxPackageInstallInstructions,
  ReleaseBuildListResult,
  UpdateCheckOptions,
  UpdateStatus
} from '../../../shared/update-status-types'
import { getVersionChannel, type ReleaseChannel } from '../../../shared/release-channel'
import { createDesktopInvoke, isShutdownRequest } from './desktop-client'
import {
  isNativeUpdateStatus,
  mapNativeUpdateStatus,
  type NativeUpdateStatus
} from './tauri-desktop-updater-capability'

// ─── API surface (verbatim from src/preload/api/updater-api.ts) ──────────────

export type UpdaterApi = {
  getVersion: () => Promise<string>
  getStatus: () => Promise<UpdateStatus>
  check: (options?: UpdateCheckOptions) => Promise<void>
  download: () => Promise<void>
  quitAndInstall: () => Promise<void>
  dismissNudge: () => Promise<void>
  dismissAvailableUpdate: () => Promise<void>
  /** Desktop-only. Rejects unless the current status carries `linux-package-install` recovery. */
  getLinuxPackageInstallInstructions: () => Promise<LinuxPackageInstallInstructions>
  /** Desktop-only. Reveals the revalidated cached package in the native file manager. */
  showLinuxPackage: () => Promise<void>
  listBuilds: (channel: ReleaseChannel) => Promise<ReleaseBuildListResult>

  onStatus: (callback: (status: UpdateStatus) => void) => () => void
  onClearDismissal: (callback: () => void) => () => void
}

type UpdaterInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

type UpdaterEventSource = (
  event: string,
  listener: (event: { payload: unknown }) => void
) => Promise<() => void>

/**
 * The native updater emits status on `mcode://updater/status`
 * (updater_commands.rs UPDATE_STATUS_EVENT) and install aborts on
 * `mcode://updater/install-aborted`. The Electron preload's
 * `updater:clearDismissal` channel (preload/index.ts:3281-3284) keeps its
 * original literal as the Tauri event name — the same discipline that kept
 * `fs://changed` unchanged.
 */
export const UPDATER_CLEAR_DISMISSAL_EVENT = 'updater:clearDismissal'

/**
 * The native updater commands serialize `Option` error-context fields as JSON
 * `null`, but the shared `DesktopCommandError` guard treats only `undefined`
 * as absent. Nulls are stripped so the payload keeps its native code — the
 * exact mechanism `fs-client.ts` uses for the same Rust boundary.
 */
function stripNullContextFields(error: unknown): unknown {
  if (
    !error ||
    typeof error !== 'object' ||
    !('context' in error) ||
    !error.context ||
    typeof error.context !== 'object' ||
    Array.isArray(error.context)
  ) {
    return error
  }
  const { context, ...rest } = error
  const cleaned = Object.fromEntries(Object.entries(context).filter(([, value]) => value !== null))
  return { ...rest, context: cleaned }
}

function validateStatus(value: unknown, operation: string): NativeUpdateStatus {
  if (!isNativeUpdateStatus(value)) {
    throw new Error(`Invalid ${operation} response`)
  }
  return value
}

function validateInstallRequest(value: unknown): unknown {
  if (!isShutdownRequest(value) || value.intent !== 'update_install') {
    throw new Error('Invalid update_install response')
  }
  return value
}

/**
 * Resolves the channel for a check exactly like
 * `createTauriDesktopUpdaterCapability` (tauri-desktop-updater-capability.ts:199-213):
 * an explicit channel wins, then includePrerelease, then the current app
 * version's channel with a stable fallback.
 */
async function resolveChannel(
  options: UpdateCheckOptions,
  getCurrentVersion: () => Promise<string>
): Promise<ReleaseChannel> {
  if (options.channel) {
    return options.channel
  }
  if (options.includePrerelease === true) {
    return 'rc'
  }
  if (options.includePrerelease === false) {
    return 'stable'
  }
  return getVersionChannel(await getCurrentVersion()) ?? 'stable'
}

/**
 * Builds the typed updater API over any invoke transport. Raw command strings
 * live only here; every failure is normalized onto `DesktopCommandError` via
 * `createDesktopInvoke`.
 */
export function createTauriUpdaterClient(
  invoke: UpdaterInvoke,
  listen: UpdaterEventSource,
  getCurrentVersion: () => Promise<string>,
  onInstallAborted: () => void
): UpdaterApi {
  const desktopInvoke = createDesktopInvoke((command, args) =>
    invoke(command, args).catch((error: unknown) => {
      throw stripNullContextFields(error)
    })
  )
  const call = (command: string, args?: Record<string, unknown>): Promise<unknown> =>
    desktopInvoke(command, args)

  return {
    getVersion: () => call('update_get_version').then((version) => version as string),
    getStatus: async () =>
      mapNativeUpdateStatus(validateStatus(await call('update_get_status'), 'update_get_status')),
    async check(options: UpdateCheckOptions = {}) {
      // assertSupportedCheckOptions (tauri-desktop-updater-capability.ts:185-197).
      if (options.targetTag) {
        throw new Error('Pinned update builds are not available through the native updater yet.')
      }
      if (options.localBuild) {
        throw new Error('Local update builds are not available through the native updater yet.')
      }
      if (options.includePerfPrerelease) {
        throw new Error(
          'Performance prerelease checks are not available through the native updater yet.'
        )
      }
      const channel = await resolveChannel(options, getCurrentVersion)
      validateStatus(
        await call('update_check', { request: { channel, userInitiated: true } }),
        'update_check'
      )
    },
    async download() {
      validateStatus(await call('update_download'), 'update_download')
    },
    async quitAndInstall() {
      validateInstallRequest(await call('update_install'))
    },
    async dismissNudge() {
      await call('update_dismiss_nudge')
    },
    async dismissAvailableUpdate() {
      await call('update_dismiss_available_update')
    },
    async getLinuxPackageInstallInstructions() {
      return call(
        'update_get_linux_package_install_instructions'
      ) as Promise<LinuxPackageInstallInstructions>
    },
    async showLinuxPackage() {
      await call('update_show_linux_package')
    },
    listBuilds: (channel) =>
      call('update_list_builds', { channel }) as Promise<ReleaseBuildListResult>,
    onStatus(listener) {
      // The preload contract is SYNCHronous: `onStatus` returns the
      // unsubscribe function directly (updater-api.ts:23; preload
      // index.ts:3278-3279). Tauri's listen is async, so the returned
      // unsubscribe is armed as each subscription resolves; a call before
      // that is a no-op. Events fired before arming are dropped by design —
      // the same boot window Electron's ipcRenderer.on already has.
      let unsubscribe: () => void = () => undefined
      void Promise.all([
        listen('mcode://updater/status', (event) => {
          if (!isNativeUpdateStatus(event.payload)) {
            console.error('[updater] Ignored invalid native updater status')
            return
          }
          if (event.payload.state === 'error' && event.payload.operation === 'install') {
            onInstallAborted()
          }
          listener(mapNativeUpdateStatus(event.payload))
        }),
        listen('mcode://updater/install-aborted', (event) => {
          try {
            validateInstallRequest(event.payload)
            onInstallAborted()
          } catch {
            console.error('[updater] Ignored invalid native updater install-aborted event')
          }
        })
      ]).then(([unsubscribeStatus, unsubscribeInstallAbort]) => {
        unsubscribe = () => {
          unsubscribeStatus()
          unsubscribeInstallAbort()
        }
      })
      return () => unsubscribe()
    },
    onClearDismissal(listener) {
      // Electron's main process sends `updater:clearDismissal` when a nudge
      // campaign applies (orca/src/main/updater.ts:2112); the preload listens
      // on that literal (preload/index.ts:3281-3284). The Tauri event keeps
      // the same name; the sync-unsubscribe adaptation matches onStatus.
      let unsubscribe: () => void = () => undefined
      void listen(UPDATER_CLEAR_DISMISSAL_EVENT, () => listener()).then((fn) => {
        unsubscribe = fn
      })
      return () => unsubscribe()
    }
  }
}

/**
 * Runtime-selection wrapper: resolves to the Tauri-native updater client
 * lazily on first use under Tauri; under Electron the caller supplies the
 * `window.api.updater` bridge (runtime-desktop-updater-client.ts:71-78
 * pattern, extended with the extra UpdaterApi methods).
 */
export function createRuntimeUpdaterClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri
}: {
  isTauri: () => boolean
  electron: UpdaterApi
  loadTauri: () => Promise<UpdaterApi>
}): UpdaterApi {
  let tauriClient: Promise<UpdaterApi> | undefined
  const resolveClient = (): Promise<UpdaterApi> => {
    if (!isTauriRuntime()) {
      return Promise.resolve(electron)
    }
    tauriClient ??= loadTauri()
    return tauriClient
  }
  return {
    getVersion: async () => (await resolveClient()).getVersion(),
    getStatus: async () => (await resolveClient()).getStatus(),
    check: async (options) => {
      await (await resolveClient()).check(options)
    },
    download: async () => {
      await (await resolveClient()).download()
    },
    quitAndInstall: async () => {
      await (await resolveClient()).quitAndInstall()
    },
    dismissNudge: async () => {
      await (await resolveClient()).dismissNudge()
    },
    dismissAvailableUpdate: async () => {
      await (await resolveClient()).dismissAvailableUpdate()
    },
    getLinuxPackageInstallInstructions: async () =>
      (await resolveClient()).getLinuxPackageInstallInstructions(),
    showLinuxPackage: async () => {
      await (await resolveClient()).showLinuxPackage()
    },
    listBuilds: async (channel) => (await resolveClient()).listBuilds(channel),
    onStatus: (listener) => {
      // Sync contract: subscribe immediately against the already-loading
      // client promise chain; the unsubscribe is sync from the caller's view.
      const client = isTauriRuntime() ? undefined : electron
      if (client) {
        return client.onStatus(listener)
      }
      let unsubscribe: () => void = () => undefined
      void loadTauri().then((tauri) => {
        unsubscribe = tauri.onStatus(listener)
      })
      return () => unsubscribe()
    },
    onClearDismissal: (listener) => {
      const client = isTauriRuntime() ? undefined : electron
      if (client) {
        return client.onClearDismissal(listener)
      }
      let unsubscribe: () => void = () => undefined
      void loadTauri().then((tauri) => {
        unsubscribe = tauri.onClearDismissal(listener)
      })
      return () => unsubscribe()
    }
  }
}

/**
 * The Electron branch handed to `createRuntimeUpdaterClient`: a verbatim
 * `window.api.updater` passthrough for every UpdaterApi method
 * (runtime-desktop-updater-client.ts:71-78, extended).
 */
export function createElectronUpdaterClient(): UpdaterApi {
  return {
    getVersion: () => window.api.updater.getVersion(),
    getStatus: () => window.api.updater.getStatus(),
    check: (options) => window.api.updater.check(options),
    download: () => window.api.updater.download(),
    quitAndInstall: () => window.api.updater.quitAndInstall(),
    dismissNudge: () => window.api.updater.dismissNudge(),
    dismissAvailableUpdate: () => window.api.updater.dismissAvailableUpdate(),
    getLinuxPackageInstallInstructions: () =>
      window.api.updater.getLinuxPackageInstallInstructions(),
    showLinuxPackage: () => window.api.updater.showLinuxPackage(),
    listBuilds: (channel) => window.api.updater.listBuilds(channel),
    onStatus: (listener) => window.api.updater.onStatus(listener),
    onClearDismissal: (listener) => window.api.updater.onClearDismissal(listener)
  }
}
