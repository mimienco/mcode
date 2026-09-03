import assert from 'node:assert/strict'
import { describe, expect, it as test, vi } from 'vitest'
import {
  UPDATER_CLEAR_DISMISSAL_EVENT,
  createElectronUpdaterClient,
  createRuntimeUpdaterClient,
  createTauriUpdaterClient,
  type UpdaterApi
} from './updater-client'
import { DesktopCommandError } from './desktop-client'
import type { NativeUpdateStatus } from './tauri-desktop-updater-capability'
import type { UpdateStatus } from '../../../shared/update-status-types'

function recordedInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler(args ?? {})
  }
  return { calls, invoke }
}

const NATIVE_STATUS: NativeUpdateStatus = {
  state: 'downloading',
  channel: 'rc',
  version: '1.4.161',
  downloadedBytes: 500,
  totalBytes: 1000,
  percent: 50
}

function fullTauriClient(
  invokeOverride?: (command: string, args?: Record<string, unknown>) => Promise<unknown>
): { client: UpdaterApi; calls: { command: string; args?: Record<string, unknown> }[] } {
  const handlers: Record<string, unknown> = {
    update_get_version: () => '9.9.9',
    update_get_status: () => NATIVE_STATUS,
    update_check: () => NATIVE_STATUS,
    update_download: () => NATIVE_STATUS,
    update_install: () => ({ version: 1, requestId: '7', intent: 'update_install' }),
    update_dismiss_nudge: () => null,
    update_dismiss_available_update: () => null,
    update_get_linux_package_install_instructions: () => null,
    update_show_linux_package: () => null,
    update_list_builds: () => ({
      ok: true,
      channel: 'stable',
      builds: [
        {
          tag: 'v1.4.160',
          version: '1.4.160',
          channel: 'stable',
          name: null,
          publishedAt: null,
          releaseUrl: 'u',
          installerUrl: null
        }
      ]
    })
  }
  const { calls, invoke } = recordedInvoke(
    handlers as Record<string, (args: Record<string, unknown>) => unknown>
  )
  const client = createTauriUpdaterClient(
    invokeOverride ?? invoke,
    async () => () => undefined,
    async () => '1.4.160',
    () => undefined
  )
  return { client, calls }
}

test('typed client maps one method onto each updater command with exact arg shapes', async () => {
  const { client, calls } = fullTauriClient()

  assert.equal(await client.getVersion(), '9.9.9')
  await client.getStatus()
  await client.check({ channel: 'daily' })
  await client.check()
  await client.download()
  await client.quitAndInstall()
  await client.dismissNudge()
  await client.dismissAvailableUpdate()
  await client.showLinuxPackage()
  await client.listBuilds('adhoc')

  assert.deepEqual(
    calls.map(({ command }) => command),
    [
      'update_get_version',
      'update_get_status',
      'update_check',
      'update_check',
      'update_download',
      'update_install',
      'update_dismiss_nudge',
      'update_dismiss_available_update',
      'update_show_linux_package',
      'update_list_builds'
    ]
  )
  assert.deepEqual(calls[2]?.args, { request: { channel: 'daily', userInitiated: true } })
  // includePrerelease=true maps to the rc channel, version-derived fallback to stable.
  assert.deepEqual(calls[3]?.args, { request: { channel: 'stable', userInitiated: true } })
  assert.deepEqual(calls[9]?.args, { channel: 'adhoc' })
})

test('void commands return undefined instead of leaking raw payloads', async () => {
  const { client } = fullTauriClient()
  assert.equal(await client.download(), undefined)
  assert.equal(await client.quitAndInstall(), undefined)
  assert.equal(await client.dismissNudge(), undefined)
  assert.equal(await client.dismissAvailableUpdate(), undefined)
  assert.equal(await client.showLinuxPackage(), undefined)
})

test('check rejects unsupported options before any invoke', async () => {
  const { client, calls } = fullTauriClient()
  await assert.rejects(
    client.check({ targetTag: 'v1.4.100' }),
    /Pinned update builds are not available/
  )
  await assert.rejects(client.check({ localBuild: true }), /Local update builds are not available/)
  await assert.rejects(
    client.check({ includePerfPrerelease: true }),
    /Performance prerelease checks are not available/
  )
  assert.deepEqual(calls, [])
})

test('invoke failures normalize onto DesktopCommandError with the native code', async () => {
  const { client } = fullTauriClient(async () => {
    throw {
      code: 'update_linux_package_unavailable',
      context: {
        operation: 'update_show_linux_package',
        intent: null
      },
      message: 'No package install recovery is available.',
      userMessage: 'No package install recovery is available.'
    }
  })
  const error = await client.showLinuxPackage().catch((caught: unknown) => caught)
  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'update_linux_package_unavailable')
  assert.equal(error.context.operation, 'update_show_linux_package')
})

test('onStatus maps native payloads and ignores invalid ones', async () => {
  const listeners: ((event: { payload: unknown }) => void)[] = []
  let unlistenCalls = 0
  const mapped: UpdateStatus[] = []
  const installAborted = vi.fn()
  const { invoke } = recordedInvoke({ update_get_version: () => '1.0.0' })
  const client = createTauriUpdaterClient(
    invoke,
    async (_event, listener) => {
      listeners.push(listener)
      return () => {
        unlistenCalls += 1
      }
    },
    async () => '1.0.0',
    installAborted
  )

  // onStatus returns the sync unsubscribe per the preload contract; the
  // Tauri subscriptions arm on the microtask queue, so flush before
  // asserting registration or calling unsubscribe.
  const unsubscribe = client.onStatus((status) => mapped.push(status))
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(listeners.length, 2)

  listeners[0]?.({ payload: NATIVE_STATUS })
  listeners[0]?.({ payload: { state: 'garbage' } })
  listeners[0]?.({
    payload: {
      state: 'error',
      operation: 'install',
      code: 'install_failed',
      message: 'boom',
      retryable: false,
      channel: null,
      version: null,
      userInitiated: true
    }
  })

  assert.deepEqual(mapped, [
    { state: 'downloading', percent: 50, version: '1.4.161' },
    { state: 'error', message: 'boom', userInitiated: true }
  ])
  expect(installAborted).toHaveBeenCalledTimes(1)

  const installAbortListener = listeners[1]
  installAbortListener?.({ payload: { version: 1, requestId: '3', intent: 'update_install' } })
  installAbortListener?.({ payload: { broken: true } })
  expect(installAborted).toHaveBeenCalledTimes(2)

  unsubscribe()
  assert.equal(unlistenCalls, 2)
})

test('onClearDismissal subscribes to the original preload channel literal', async () => {
  const listen = vi.fn(async () => () => undefined)
  const { invoke } = recordedInvoke({ update_get_version: () => '1.0.0' })
  const client = createTauriUpdaterClient(
    invoke,
    listen,
    async () => '1.0.0',
    () => undefined
  )

  const fired: boolean[] = []
  const unsubscribe = await client.onClearDismissal(() => fired.push(true))
  expect(listen).toHaveBeenCalledWith(UPDATER_CLEAR_DISMISSAL_EVENT, expect.any(Function))
  assert.equal(UPDATER_CLEAR_DISMISSAL_EVENT, 'updater:clearDismissal')

  const [, listener] = listen.mock.calls[0] as unknown as [
    string,
    (event: { payload: unknown }) => void
  ]
  listener({ payload: null })
  assert.deepEqual(fired, [true])

  unsubscribe()
})

test('listBuilds passes failure results through as data, not rejections', async () => {
  const { client } = fullTauriClient(async (command) => {
    if (command === 'update_list_builds') {
      return {
        ok: false,
        channel: 'hourly',
        message: 'GitHub rate limit reached. Try again in a few minutes.'
      }
    }
    throw new Error(`unexpected command: ${command}`)
  })
  const result = await client.listBuilds('hourly')
  assert.deepEqual(result, {
    ok: false,
    channel: 'hourly',
    message: 'GitHub rate limit reached. Try again in a few minutes.'
  })
})

test('runtime wrapper routes to Electron without loading the Tauri client', async () => {
  let tauriLoads = 0
  let electronDismissals = 0
  const client = createRuntimeUpdaterClient({
    isTauri: () => false,
    electron: {
      dismissNudge: () => {
        electronDismissals += 1
        return Promise.resolve()
      }
    } as unknown as UpdaterApi,
    loadTauri: async () => {
      tauriLoads += 1
      throw new Error('Tauri client must not load under Electron')
    }
  })

  await client.dismissNudge()
  assert.equal(electronDismissals, 1)
  assert.equal(tauriLoads, 0)
})

test('runtime wrapper loads the Tauri client once and reuses it', async () => {
  let tauriLoads = 0
  let tauriChecks = 0
  const client = createRuntimeUpdaterClient({
    isTauri: () => true,
    electron: {
      check: () => {
        throw new Error('Electron client must not run in Tauri')
      }
    } as unknown as UpdaterApi,
    loadTauri: async () => {
      tauriLoads += 1
      return {
        check: () => {
          tauriChecks += 1
          return Promise.resolve()
        }
      } as unknown as UpdaterApi
    }
  })

  await client.check({ channel: 'rc' })
  await client.check({ channel: 'rc' })
  assert.equal(tauriLoads, 1)
  assert.equal(tauriChecks, 2)
})

test('every runtime wrapper method forwards args verbatim', async () => {
  const tauri: UpdaterApi = {
    getVersion: async () => '1.0.0',
    getStatus: async () => ({ state: 'idle' }),
    check: async () => undefined,
    download: async () => undefined,
    quitAndInstall: async () => undefined,
    dismissNudge: async () => undefined,
    dismissAvailableUpdate: async () => undefined,
    getLinuxPackageInstallInstructions: async () => ({
      ok: true,
      command: 'sudo apt install',
      packageFileName: 'p.deb'
    }),
    showLinuxPackage: async () => undefined,
    listBuilds: async () => ({ ok: true, channel: 'stable', builds: [] }),
    onStatus: () => () => undefined,
    onClearDismissal: () => () => undefined
  }
  const client = createRuntimeUpdaterClient({
    isTauri: () => true,
    electron: {} as unknown as UpdaterApi,
    loadTauri: async () => tauri
  })

  assert.equal(await client.getVersion(), '1.0.0')
  assert.deepEqual(await client.getStatus(), { state: 'idle' })
  await client.check({ channel: 'rc' })
  await client.download()
  await client.quitAndInstall()
  await client.dismissNudge()
  await client.dismissAvailableUpdate()
  assert.deepEqual(await client.getLinuxPackageInstallInstructions(), {
    ok: true,
    command: 'sudo apt install',
    packageFileName: 'p.deb'
  })
  await client.showLinuxPackage()
  assert.deepEqual(await client.listBuilds('rc'), { ok: true, channel: 'stable', builds: [] })
  await client.onClearDismissal(() => undefined)
})

test('electron passthrough client delegates every method to the supplied window bridge', async () => {
  const calls: string[] = []
  const windowBridge = {
    getVersion: () => {
      calls.push('getVersion')
      return Promise.resolve('2.0.0')
    },
    getStatus: () => {
      calls.push('getStatus')
      return Promise.resolve({ state: 'idle' })
    },
    check: () => {
      calls.push('check')
      return Promise.resolve()
    },
    download: () => {
      calls.push('download')
      return Promise.resolve()
    },
    quitAndInstall: () => {
      calls.push('quitAndInstall')
      return Promise.resolve()
    },
    dismissNudge: () => {
      calls.push('dismissNudge')
      return Promise.resolve()
    },
    dismissAvailableUpdate: () => {
      calls.push('dismissAvailableUpdate')
      return Promise.resolve()
    },
    getLinuxPackageInstallInstructions: () => {
      calls.push('getLinuxPackageInstallInstructions')
      return Promise.resolve({ ok: false, reason: 'no-sudo', message: 'm' })
    },
    showLinuxPackage: () => {
      calls.push('showLinuxPackage')
      return Promise.resolve()
    },
    listBuilds: () => {
      calls.push('listBuilds')
      return Promise.resolve({ ok: true, channel: 'stable', builds: [] })
    },
    onStatus: () => {
      calls.push('onStatus')
      return () => undefined
    },
    onClearDismissal: () => {
      calls.push('onClearDismissal')
      return () => undefined
    }
  } as unknown as typeof window.api.updater
  const globalWindow = globalThis as { window?: unknown }
  const previousWindow = globalWindow.window
  globalWindow.window = { api: { updater: windowBridge } }
  try {
    const client = createElectronUpdaterClient()
    await client.getVersion()
    await client.getStatus()
    await client.check({ channel: 'rc' })
    await client.download()
    await client.quitAndInstall()
    await client.dismissNudge()
    await client.dismissAvailableUpdate()
    await client.getLinuxPackageInstallInstructions()
    await client.showLinuxPackage()
    await client.listBuilds('stable')
    client.onStatus(() => undefined)
    client.onClearDismissal(() => undefined)
  } finally {
    globalWindow.window = previousWindow
  }

  assert.deepEqual(calls, [
    'getVersion',
    'getStatus',
    'check',
    'download',
    'quitAndInstall',
    'dismissNudge',
    'dismissAvailableUpdate',
    'getLinuxPackageInstallInstructions',
    'showLinuxPackage',
    'listBuilds',
    'onStatus',
    'onClearDismissal'
  ])
})

describe('smoke', () => {
  test('clear-dismissal event name matches the preload channel', () => {
    expect(UPDATER_CLEAR_DISMISSAL_EVENT).toBe('updater:clearDismissal')
  })
})
