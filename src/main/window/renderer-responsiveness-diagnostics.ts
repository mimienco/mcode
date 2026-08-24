import { app, type BrowserWindow } from 'electron'
import type { CrashReportBreadcrumbData } from '../../shared/crash-reporting'
import { recordDurableCrashBreadcrumb } from '../crash-reporting/durable-crash-breadcrumb'

type RendererResponsivenessTarget = Pick<BrowserWindow, 'on' | 'webContents'>

type RendererResponsivenessDiagnosticsOptions = {
  now?: () => number
  record?: (name: string, data: CrashReportBreadcrumbData) => void
}

function recordRendererObservation(
  record: (name: string, data: CrashReportBreadcrumbData) => void,
  name: string,
  data: CrashReportBreadcrumbData
): void {
  try {
    record(name, data)
  } catch (error) {
    // Diagnostics must never turn a renderer stall into a main-process failure.
    console.error(`[renderer-stall] failed to record ${name}`, error)
  }
}

function rendererProcessId(target: RendererResponsivenessTarget): number | undefined {
  try {
    const getOSProcessId = target.webContents?.getOSProcessId
    return typeof getOSProcessId === 'function'
      ? getOSProcessId.call(target.webContents)
      : undefined
  } catch {
    return undefined
  }
}

function rendererProcessMetrics(rendererPid: number | undefined): CrashReportBreadcrumbData {
  if (rendererPid === undefined) {
    return {}
  }
  try {
    const metric = app.getAppMetrics().find((candidate) => candidate.pid === rendererPid)
    if (!metric) {
      return {}
    }
    const privateBytes = metric.memory.privateBytes
    return {
      cpuPercent: Math.round(metric.cpu.percentCPUUsage * 10) / 10,
      workingSetMB: Math.round(metric.memory.workingSetSize / 1024),
      ...(typeof privateBytes === 'number'
        ? { privateMemoryMB: Math.round(privateBytes / 1024) }
        : {})
    }
  } catch {
    return {}
  }
}

function rendererObservationData(
  target: RendererResponsivenessTarget,
  data: CrashReportBreadcrumbData
): CrashReportBreadcrumbData {
  const rendererPid = rendererProcessId(target)
  return {
    ...data,
    webContentsId: target.webContents.id,
    rendererPid: rendererPid ?? null,
    ...rendererProcessMetrics(rendererPid)
  }
}

/** Records the exact native window events behind Windows' "Not responding" title. */
export function installRendererResponsivenessDiagnostics(
  target: RendererResponsivenessTarget,
  options: RendererResponsivenessDiagnosticsOptions = {}
): void {
  const now = options.now ?? Date.now
  const record = options.record ?? recordDurableCrashBreadcrumb
  let unresponsiveAt: number | null = null
  let episode = 0

  target.on('unresponsive', () => {
    if (unresponsiveAt !== null) {
      return
    }
    unresponsiveAt = now()
    episode++
    const data = rendererObservationData(target, { episode })
    recordRendererObservation(record, 'renderer_unresponsive', data)
    console.warn('[renderer-stall] renderer became unresponsive', data)
  })

  target.on('responsive', () => {
    if (unresponsiveAt === null) {
      return
    }
    const durationMs = Math.max(0, now() - unresponsiveAt)
    unresponsiveAt = null
    const data = rendererObservationData(target, {
      episode,
      durationMs
    })
    recordRendererObservation(record, 'renderer_responsive', data)
    console.warn('[renderer-stall] renderer recovered', data)
  })
}
