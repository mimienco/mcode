import type { ReleaseChannel } from '../../../shared/release-channel'
import { getVersionChannel, isReleaseChannel } from '../../../shared/release-channel'
import type {
  UpdateCheckOptions,
  UpdateSource,
  UpdateStatus
} from '../../../shared/update-status-types'
import type { DesktopInvoke, ShutdownRequest } from './desktop-client'
import { isShutdownRequest } from './desktop-client'
import type { TauriListen } from './tauri-desktop-app-lifecycle-capability'

export type NativeUpdateOperation = 'check' | 'download' | 'install'

export type NativeUpdateStatus =
  | { state: 'idle' }
  | { state: 'checking'; channel: ReleaseChannel; userInitiated: boolean }
  | {
      state: 'available'
      channel: ReleaseChannel
      currentVersion: string
      version: string
      body: string | null
      publishedAt: string | null
      userInitiated: boolean
    }
  | { state: 'not-available'; channel: ReleaseChannel; userInitiated: boolean }
  | {
      state: 'downloading'
      channel: ReleaseChannel
      version: string
      downloadedBytes: number
      totalBytes: number | null
      percent: number | null
    }
  | { state: 'downloaded'; channel: ReleaseChannel; version: string }
  | { state: 'installing'; channel: ReleaseChannel; version: string }
  | {
      state: 'error'
      operation: NativeUpdateOperation
      code: string
      message: string
      retryable: boolean
      channel: ReleaseChannel | null
      version: string | null
      userInitiated: boolean
    }

export type DesktopUpdaterCoreCapability = {
  getVersion: () => Promise<string>
  getStatus: () => Promise<UpdateStatus>
  check: (options?: UpdateCheckOptions) => Promise<void>
  checkOnLaunch?: () => Promise<void>
  download: () => Promise<void>
  quitAndInstall: () => Promise<void>
  onStatus: (listener: (status: UpdateStatus) => void) => Promise<() => void>
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object'
}

function hasChannel(value: Record<string, unknown>): boolean {
  return isReleaseChannel(value.channel)
}

function isNullableString(value: unknown): boolean {
  return value === null || typeof value === 'string'
}

function isNullableNumber(value: unknown): boolean {
  return value === null || (typeof value === 'number' && Number.isFinite(value))
}

export function isNativeUpdateStatus(value: unknown): value is NativeUpdateStatus {
  if (!isRecord(value) || typeof value.state !== 'string') {
    return false
  }
  switch (value.state) {
    case 'idle':
      return true
    case 'checking':
    case 'not-available':
      return hasChannel(value) && typeof value.userInitiated === 'boolean'
    case 'available':
      return (
        hasChannel(value) &&
        typeof value.currentVersion === 'string' &&
        typeof value.version === 'string' &&
        isNullableString(value.body) &&
        isNullableString(value.publishedAt) &&
        typeof value.userInitiated === 'boolean'
      )
    case 'downloading':
      return (
        hasChannel(value) &&
        typeof value.version === 'string' &&
        typeof value.downloadedBytes === 'number' &&
        Number.isFinite(value.downloadedBytes) &&
        isNullableNumber(value.totalBytes) &&
        isNullableNumber(value.percent)
      )
    case 'downloaded':
    case 'installing':
      return hasChannel(value) && typeof value.version === 'string'
    case 'error':
      return (
        (value.operation === 'check' ||
          value.operation === 'download' ||
          value.operation === 'install') &&
        typeof value.code === 'string' &&
        typeof value.message === 'string' &&
        typeof value.retryable === 'boolean' &&
        (value.channel === null || isReleaseChannel(value.channel)) &&
        isNullableString(value.version) &&
        typeof value.userInitiated === 'boolean'
      )
    default:
      return false
  }
}

function sourceForChannel(channel: ReleaseChannel | null): UpdateSource | undefined {
  return channel === 'hourly' || channel === 'daily' || channel === 'adhoc' ? channel : undefined
}

function withSource<T extends UpdateStatus>(status: T, channel: ReleaseChannel | null): T {
  const source = sourceForChannel(channel)
  return source ? ({ ...status, source } as T) : status
}

export function mapNativeUpdateStatus(status: NativeUpdateStatus): UpdateStatus {
  switch (status.state) {
    case 'idle':
      return { state: 'idle' }
    case 'checking':
      return withSource({ state: 'checking', userInitiated: status.userInitiated }, status.channel)
    case 'available':
      return withSource(
        { state: 'available', version: status.version, changelog: null },
        status.channel
      )
    case 'not-available':
      return withSource(
        { state: 'not-available', userInitiated: status.userInitiated },
        status.channel
      )
    case 'downloading':
      return withSource(
        {
          state: 'downloading',
          version: status.version,
          percent: status.percent ?? 0
        },
        status.channel
      )
    case 'downloaded':
    case 'installing':
      return withSource({ state: 'downloaded', version: status.version }, status.channel)
    case 'error':
      return withSource(
        {
          state: 'error',
          message: status.message,
          userInitiated: status.userInitiated
        },
        status.channel
      )
  }
}

function validateStatus(value: unknown, operation: string): NativeUpdateStatus {
  if (!isNativeUpdateStatus(value)) {
    throw new Error(`Invalid ${operation} response`)
  }
  return value
}

function validateInstallRequest(value: unknown): ShutdownRequest {
  if (!isShutdownRequest(value) || value.intent !== 'update_install') {
    throw new Error('Invalid update_install response')
  }
  return value
}

function assertSupportedCheckOptions(options: UpdateCheckOptions): void {
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
}

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

export function createTauriDesktopUpdaterCapability(
  invoke: DesktopInvoke,
  listen: TauriListen,
  getCurrentVersion: () => Promise<string>,
  onInstallAborted: () => void
): DesktopUpdaterCoreCapability {
  const check = async (options: UpdateCheckOptions, userInitiated: boolean): Promise<void> => {
    assertSupportedCheckOptions(options)
    const channel = await resolveChannel(options, getCurrentVersion)
    validateStatus(
      await invoke('update_check', {
        request: { channel, userInitiated }
      }),
      'update_check'
    )
  }

  return {
    getVersion: getCurrentVersion,
    async getStatus() {
      return mapNativeUpdateStatus(
        validateStatus(await invoke('update_get_status'), 'update_get_status')
      )
    },
    check: (options = {}) => check(options, true),
    checkOnLaunch: () => check({}, false),
    async download() {
      validateStatus(await invoke('update_download'), 'update_download')
    },
    async quitAndInstall() {
      validateInstallRequest(await invoke('update_install'))
    },
    async onStatus(listener) {
      const [unsubscribeStatus, unsubscribeInstallAbort] = await Promise.all([
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
      ])
      return () => {
        unsubscribeStatus()
        unsubscribeInstallAbort()
      }
    }
  }
}
