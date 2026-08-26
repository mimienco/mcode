import { ipcMain, type BrowserWindow, type IpcMainEvent } from 'electron'
import type { RendererRecoveryCircuitBreaker } from '../crash-reporting/renderer-recovery-circuit-breaker'
import { recordDurableCrashBreadcrumb } from '../crash-reporting/durable-crash-breadcrumb'
import {
  RENDERER_HEALTH_CHANNEL,
  RENDERER_HEALTH_PROBE_CHANNEL,
  type RendererHealthReport
} from '../../shared/renderer-bootstrap-health'

export const DEV_RENDERER_HEALTH_TIMEOUT_MS = 5_000
const ELECTRON_ERR_ABORTED = -3

type HealthCheckTrigger = 'load' | 'load-failure' | 'resume' | 'root-empty'

type DevRendererHealthRecoveryOptions = {
  enabled: boolean
  window: BrowserWindow
  isClosing: () => boolean
  reload: () => void
  recoveryCircuitBreaker: RendererRecoveryCircuitBreaker
  onBeforeReload?: (webContentsId: number) => void
}

export type DevRendererHealthRecovery = {
  navigationStarted: () => void
  documentLoaded: () => void
  documentLoadFailed: (errorCode: number, isMainFrame: boolean) => void
  systemResumed: () => void
  dispose: () => void
}

export function installDevRendererHealthRecovery({
  enabled,
  window,
  isClosing,
  reload,
  recoveryCircuitBreaker,
  onBeforeReload
}: DevRendererHealthRecoveryOptions): DevRendererHealthRecovery {
  let healthy = false
  let disposed = false
  let timer: ReturnType<typeof setTimeout> | null = null
  let activeProbeId: number | null = null
  let nextProbeId = 1
  const webContentsId = window.webContents.id

  const clearTimer = (): void => {
    if (timer) {
      clearTimeout(timer)
      timer = null
    }
  }
  const arm = (trigger: HealthCheckTrigger): void => {
    if (!enabled || disposed || healthy || timer) {
      return
    }
    timer = setTimeout(() => {
      timer = null
      if (healthy || disposed || isClosing() || window.isDestroyed()) {
        return
      }
      const recovery = recoveryCircuitBreaker.registerRecoveryAttempt(Date.now())
      if (!recovery.allowed) {
        recordDurableCrashBreadcrumb('dev_renderer_health_recovery_exhausted', {
          trigger,
          recentRecoveryCount: recovery.recentRecoveryCount
        })
        console.error('[window] Dev renderer health recovery exhausted', { trigger })
        return
      }
      recordDurableCrashBreadcrumb('dev_renderer_health_recovery', {
        trigger,
        recentRecoveryCount: recovery.recentRecoveryCount
      })
      onBeforeReload?.(webContentsId)
      reload()
    }, DEV_RENDERER_HEALTH_TIMEOUT_MS)
    timer.unref?.()
  }
  const startProbe = (trigger: HealthCheckTrigger): void => {
    if (!enabled || disposed) {
      return
    }
    healthy = false
    activeProbeId = nextProbeId
    nextProbeId += 1
    arm(trigger)
    if (!window.isDestroyed() && !window.webContents.isDestroyed()) {
      window.webContents.send(RENDERER_HEALTH_PROBE_CHANNEL, activeProbeId)
    }
  }
  const onRendererHealth = (event: IpcMainEvent, value: unknown): void => {
    if (
      event.sender.id !== webContentsId ||
      !value ||
      typeof value !== 'object' ||
      typeof (value as Partial<RendererHealthReport>).healthy !== 'boolean' ||
      (value as Partial<RendererHealthReport>).probeId !== activeProbeId
    ) {
      return
    }
    healthy = (value as RendererHealthReport).healthy
    if (healthy) {
      clearTimer()
    } else {
      arm('root-empty')
    }
  }

  if (enabled) {
    ipcMain.on(RENDERER_HEALTH_CHANNEL, onRendererHealth)
  }

  return {
    navigationStarted: () => {
      healthy = false
      activeProbeId = null
      clearTimer()
    },
    documentLoaded: () => startProbe('load'),
    documentLoadFailed: (errorCode, isMainFrame) => {
      if (isMainFrame && errorCode !== ELECTRON_ERR_ABORTED) {
        healthy = false
        arm('load-failure')
      }
    },
    systemResumed: () => {
      startProbe('resume')
    },
    dispose: () => {
      disposed = true
      clearTimer()
      if (enabled) {
        ipcMain.removeListener(RENDERER_HEALTH_CHANNEL, onRendererHealth)
      }
    }
  }
}
