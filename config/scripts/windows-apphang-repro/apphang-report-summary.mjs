import { existsSync, readFileSync } from 'node:fs'
import path from 'node:path'

export function summarizeTrace(userDataDir) {
  const tracePath = path.join(userDataDir, 'logs', 'main.trace.ndjson')
  if (!existsSync(tracePath)) {
    return { tracePath, exists: false }
  }
  const lines = readFileSync(tracePath, 'utf8').trim().split(/\r?\n/).filter(Boolean)
  const parsed = []
  for (const line of lines.slice(-400)) {
    try {
      parsed.push(JSON.parse(line))
    } catch {
      parsed.push({ raw: line })
    }
  }
  const stallNames = new Set([
    'renderer_event_loop_stall',
    'renderer_unresponsive',
    'renderer_responsive',
    'main_thread_unresponsive',
    'main_thread_responsive',
    'main_thread_hang_detected'
  ])
  const stallEvents = parsed.flatMap((entry) => {
    const name = entry.attributes?.['breadcrumb.name']
    if (!stallNames.has(name)) {
      return []
    }
    return [
      { name, data: entry.attributes?.['breadcrumb.data'] ?? {}, durationMs: entry.durationMs }
    ]
  })
  const interesting = parsed.filter((entry) => {
    const name = String(entry.name ?? entry.event ?? entry.type ?? '')
    const text = JSON.stringify(entry)
    return (
      name.includes('git.exec') ||
      name.includes('renderer_memory') ||
      name.includes('sidebar_worktree_activate') ||
      stallNames.has(entry.attributes?.['breadcrumb.name']) ||
      text.includes('git.exec remote') ||
      text.includes('git.exec worktree') ||
      text.includes('sidebar_worktree_activate')
    )
  })
  return {
    tracePath,
    exists: true,
    totalLines: lines.length,
    stallSummary: {
      count: stallEvents.length,
      byName: Object.fromEntries(
        [...stallNames].map((name) => [
          name,
          stallEvents.filter((event) => event.name === name).length
        ])
      ),
      maxRendererBlockedMs: Math.max(0, ...stallEvents.map((event) => event.data.blockedMs ?? 0)),
      maxNativeRendererUnresponsiveMs: Math.max(
        0,
        ...stallEvents
          .filter((event) => event.name === 'renderer_responsive')
          .map((event) => event.data.durationMs ?? 0)
      ),
      maxMainUnresponsiveMs: Math.max(
        0,
        ...stallEvents
          .filter((event) => event.name.startsWith('main_thread_'))
          .map((event) => event.data.unresponsiveMs ?? 0)
      )
    },
    interestingTail: interesting.slice(-40)
  }
}

export function summarizeAppLogs(logs) {
  const webglContextWarnings = logs.filter((entry) =>
    entry.line.includes('Too many active WebGL contexts')
  )
  const webglContextLosses = logs.filter((entry) =>
    entry.line.toLowerCase().includes('webgl context lost')
  )
  const gpuProcessEvents = logs.filter((entry) => /gpu|d3d|angle/i.test(entry.line))
  return {
    lineCount: logs.length,
    webglContextWarningCount: webglContextWarnings.length,
    webglContextWarningsTail: webglContextWarnings.slice(-10),
    webglContextLossCount: webglContextLosses.length,
    webglContextLossesTail: webglContextLosses.slice(-10),
    gpuProcessEventCount: gpuProcessEvents.length,
    gpuProcessEventsTail: gpuProcessEvents.slice(-20)
  }
}

export function summarizeResult(result) {
  const samples = result.samples.filter((sample) => sample && typeof sample === 'object')
  const maxActivationMs = Math.max(0, ...samples.map((sample) => sample.activationMs ?? 0))
  const maxPtyWaitMs = Math.max(0, ...samples.map((sample) => sample.ptyWaitMs ?? 0))
  const maxMainIpcMs = Math.max(0, ...samples.map((sample) => sample.mainIpcMs ?? 0))
  const maxRendererDriftMs = Math.max(0, ...samples.map((sample) => sample.rendererMaxDriftMs ?? 0))
  return {
    gpuMode: result.gpuMode,
    reproduced: result.reproduced,
    reproductionReason: result.reproductionReason,
    harnessError: result.harnessError ?? null,
    elapsedMs: result.elapsedMs,
    sampleCount: samples.length,
    maxActivationMs,
    maxPtyWaitMs,
    maxMainIpcMs,
    maxRendererDriftMs,
    finalWebglContextCounts: result.finalDiagnostics?.webglContextCounts ?? null,
    appLogSummary: result.appLogSummary
  }
}
