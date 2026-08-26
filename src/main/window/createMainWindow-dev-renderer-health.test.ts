import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('electron', async () =>
  (await import('./createMainWindow-test-harness')).electronModuleMock()
)
vi.mock('@electron-toolkit/utils', async () =>
  (await import('./createMainWindow-test-harness')).electronToolkitUtilsMock()
)
vi.mock('./macos-tahoe-release', async () =>
  (await import('./createMainWindow-test-harness')).macosTahoeReleaseMock()
)
vi.mock('../app-icon', async () => (await import('./createMainWindow-test-harness')).appIconMock())
vi.mock('../browser/browser-manager', async () =>
  (await import('./createMainWindow-test-harness')).browserManagerMock()
)

import { ipcMain } from 'electron'
import { createMainWindow } from './createMainWindow'
import { DEV_RENDERER_HEALTH_TIMEOUT_MS } from './dev-renderer-health-recovery'
import { resetExpectedTeardownStateForTest } from '../crash-reporting/expected-teardown-state'
import {
  browserWindowMock,
  isMock,
  powerMonitorOnMock,
  resetMainWindowMocks
} from './createMainWindow-test-harness'
import {
  RENDERER_HEALTH_CHANNEL,
  RENDERER_HEALTH_PROBE_CHANNEL
} from '../../shared/renderer-bootstrap-health'

function installTestWindow(webContentsId: number): {
  handlers: Record<string, (...args: any[]) => void>
  webContents: { id: number; send: ReturnType<typeof vi.fn> }
  instance: { loadURL: ReturnType<typeof vi.fn> }
} {
  const handlers: Record<string, (...args: any[]) => void> = {}
  const webContents = {
    id: webContentsId,
    on: vi.fn((event: string, handler: (...args: any[]) => void) => {
      handlers[event] = handler
    }),
    setZoomLevel: vi.fn(),
    setBackgroundThrottling: vi.fn(),
    invalidate: vi.fn(),
    setWindowOpenHandler: vi.fn(),
    send: vi.fn(),
    isDestroyed: vi.fn(() => false)
  }
  const instance = {
    webContents,
    on: vi.fn(),
    isDestroyed: vi.fn(() => false),
    isMaximized: vi.fn(() => true),
    isFullScreen: vi.fn(() => false),
    getSize: vi.fn(() => [1200, 800]),
    setSize: vi.fn(),
    maximize: vi.fn(),
    show: vi.fn(),
    loadFile: vi.fn(),
    loadURL: vi.fn(() => Promise.resolve())
  }
  browserWindowMock.mockImplementation(function () {
    return instance
  })
  return { handlers, webContents, instance }
}

function rendererHealthListener(): (...args: any[]) => void {
  const listener = vi
    .mocked(ipcMain.on)
    .mock.calls.find(([channel]) => channel === RENDERER_HEALTH_CHANNEL)?.[1]
  if (!listener) {
    throw new Error('renderer health listener was not installed')
  }
  return listener
}

function latestProbeId(webContents: { send: ReturnType<typeof vi.fn> }): number {
  const probeCall = webContents.send.mock.calls.findLast(
    ([channel]) => channel === RENDERER_HEALTH_PROBE_CHANNEL
  )
  if (typeof probeCall?.[1] !== 'number') {
    throw new Error('renderer health probe was not sent')
  }
  return probeCall[1]
}

describe('createMainWindow dev renderer health recovery', () => {
  beforeEach(() => {
    resetMainWindowMocks()
    resetExpectedTeardownStateForTest()
    vi.useFakeTimers()
    vi.stubEnv('ELECTRON_RENDERER_URL', 'http://localhost:5173')
    isMock.dev = true
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllEnvs()
  })

  it('reloads a dev renderer that does not report healthy after its document loads', () => {
    const { handlers, instance } = installTestWindow(41)
    const onBeforeRecoveryReload = vi.fn()

    createMainWindow(null, { deferLoad: true, onBeforeRecoveryReload })
    handlers['did-finish-load']?.()
    vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)

    expect(onBeforeRecoveryReload).toHaveBeenCalledWith(41)
    expect(instance.loadURL).toHaveBeenCalledWith('http://localhost:5173')
  })

  it('cancels recovery when the loaded renderer reports healthy', () => {
    const { handlers, webContents, instance } = installTestWindow(42)

    createMainWindow(null, { deferLoad: true })
    handlers['did-finish-load']?.()
    rendererHealthListener()(
      { sender: webContents },
      {
        healthy: true,
        probeId: latestProbeId(webContents)
      }
    )
    vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)

    expect(instance.loadURL).not.toHaveBeenCalled()
  })

  it('reloads when HMR leaves a previously healthy renderer root empty', () => {
    const { handlers, webContents, instance } = installTestWindow(43)

    createMainWindow(null, { deferLoad: true })
    handlers['did-finish-load']?.()
    const probeId = latestProbeId(webContents)
    rendererHealthListener()({ sender: webContents }, { healthy: true, probeId })
    rendererHealthListener()({ sender: webContents }, { healthy: false, probeId })
    vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)

    expect(instance.loadURL).toHaveBeenCalledWith('http://localhost:5173')
  })

  it('reloads a previously healthy renderer that does not re-acknowledge system resume', () => {
    const { handlers, webContents, instance } = installTestWindow(44)

    createMainWindow(null, { deferLoad: true })
    handlers['did-finish-load']?.()
    rendererHealthListener()(
      { sender: webContents },
      {
        healthy: true,
        probeId: latestProbeId(webContents)
      }
    )
    const resumeListener = powerMonitorOnMock.mock.calls.find(([event]) => event === 'resume')?.[1]
    ;(resumeListener as (() => void) | undefined)?.()
    vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)

    expect(instance.loadURL).toHaveBeenCalledWith('http://localhost:5173')
  })

  it('shares the renderer recovery circuit breaker and stops repeated blank-root reloads', () => {
    const { handlers, webContents, instance } = installTestWindow(45)

    createMainWindow(null, { deferLoad: true })
    handlers['did-finish-load']?.()
    const healthListener = rendererHealthListener()
    const probeId = latestProbeId(webContents)
    healthListener({ sender: webContents }, { healthy: true, probeId })
    for (let attempt = 0; attempt < 4; attempt += 1) {
      healthListener({ sender: webContents }, { healthy: false, probeId })
      vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)
    }

    expect(instance.loadURL).toHaveBeenCalledTimes(3)
  })

  it('ignores a stale health acknowledgement from the previous navigation', () => {
    const { handlers, webContents, instance } = installTestWindow(46)

    createMainWindow(null, { deferLoad: true })
    handlers['did-finish-load']?.()
    const staleProbeId = latestProbeId(webContents)
    handlers['did-start-navigation']?.(undefined, '', false, true)
    handlers['did-finish-load']?.()
    rendererHealthListener()({ sender: webContents }, { healthy: true, probeId: staleProbeId })
    vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)

    expect(instance.loadURL).toHaveBeenCalledWith('http://localhost:5173')
  })

  it('does not install health recovery in packaged production', () => {
    const { handlers, instance } = installTestWindow(47)
    isMock.dev = false

    createMainWindow(null, { deferLoad: true })
    handlers['did-finish-load']?.()
    vi.advanceTimersByTime(DEV_RENDERER_HEALTH_TIMEOUT_MS)

    expect(instance.loadURL).not.toHaveBeenCalled()
    expect(
      vi.mocked(ipcMain.on).mock.calls.some(([channel]) => channel === RENDERER_HEALTH_CHANNEL)
    ).toBe(false)
  })
})
