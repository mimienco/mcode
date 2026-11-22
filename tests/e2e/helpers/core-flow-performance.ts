import { mkdirSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { performance as nodePerformance } from 'node:perf_hooks'
import type { ElectronApplication, Page, TestInfo } from '@stablyai/playwright-test'

type ProcessSample = {
  capturedAtMs: number
  cpuPercent: number
  workingSetMb: number
  privateMb: number
  processCount: number
}

type StepMeasurement = {
  label: string
  startMs: number
  durationMs: number
}

type RendererResponsiveness = {
  durationMs: number
  longTasks: {
    count: number
    totalDurationMs: number
    maxDurationMs: number | null
    entries: { startMs: number; durationMs: number }[]
  }
  frameGaps: {
    countOver50Ms: number
    maxMs: number | null
    entries: { startMs: number; durationMs: number }[]
  }
}

type MetricSummary = {
  mean: number | null
  p95: number | null
  max: number | null
}

export type CoreFlowPerformanceProbe = {
  measure<T>(label: string, action: () => Promise<T>): Promise<T>
  stop(testInfo: TestInfo): Promise<void>
}

function round(value: number): number {
  return Math.round(value * 100) / 100
}

function summarize(values: number[]): MetricSummary {
  if (values.length === 0) {
    return { mean: null, p95: null, max: null }
  }
  const sorted = [...values].sort((left, right) => left - right)
  return {
    mean: round(sorted.reduce((sum, value) => sum + value, 0) / sorted.length),
    p95: round(sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)]),
    max: round(sorted.at(-1) ?? 0)
  }
}

async function readProcessSample(
  electronApp: ElectronApplication,
  startedAt: number
): Promise<ProcessSample> {
  const metrics = await electronApp.evaluate(({ app }) =>
    app.getAppMetrics().map((metric) => ({
      cpuPercent: metric.cpu.percentCPUUsage,
      workingSetKb: metric.memory.workingSetSize,
      privateKb: metric.memory.privateBytes
    }))
  )
  return {
    capturedAtMs: round(nodePerformance.now() - startedAt),
    cpuPercent: round(metrics.reduce((sum, metric) => sum + metric.cpuPercent, 0)),
    workingSetMb: round(metrics.reduce((sum, metric) => sum + metric.workingSetKb, 0) / 1024),
    privateMb: round(metrics.reduce((sum, metric) => sum + (metric.privateKb ?? 0), 0) / 1024),
    processCount: metrics.length
  }
}

async function startRendererProbe(page: Page): Promise<void> {
  await page.evaluate(() => {
    const startedAt = window.performance.now()
    const longTasks: { startMs: number; durationMs: number }[] = []
    const frameGaps: { startMs: number; durationMs: number }[] = []
    let previousFrameAt = startedAt
    let animationFrameId = 0

    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        longTasks.push({
          startMs: Math.round((entry.startTime - startedAt) * 100) / 100,
          durationMs: Math.round(entry.duration * 100) / 100
        })
      }
    })
    if (PerformanceObserver.supportedEntryTypes.includes('longtask')) {
      observer.observe({ type: 'longtask', buffered: false })
    }

    const captureFrame = (now: number): void => {
      const gap = now - previousFrameAt
      if (gap > 50) {
        frameGaps.push({
          startMs: Math.round((previousFrameAt - startedAt) * 100) / 100,
          durationMs: Math.round(gap * 100) / 100
        })
      }
      previousFrameAt = now
      animationFrameId = requestAnimationFrame(captureFrame)
    }
    animationFrameId = requestAnimationFrame(captureFrame)

    const performanceWindow = window as typeof window & {
      __mcodeCoreFlowPerformance?: { stop: () => RendererResponsiveness }
    }
    performanceWindow.__mcodeCoreFlowPerformance = {
      stop: () => {
        cancelAnimationFrame(animationFrameId)
        observer.disconnect()
        const durationMs = Math.round((performance.now() - startedAt) * 100) / 100
        const totalLongTaskMs = longTasks.reduce((sum, entry) => sum + entry.durationMs, 0)
        return {
          durationMs,
          longTasks: {
            count: longTasks.length,
            totalDurationMs: Math.round(totalLongTaskMs * 100) / 100,
            maxDurationMs:
              longTasks.length === 0
                ? null
                : Math.max(...longTasks.map((entry) => entry.durationMs)),
            entries: longTasks
          },
          frameGaps: {
            countOver50Ms: frameGaps.length,
            maxMs:
              frameGaps.length === 0
                ? null
                : Math.max(...frameGaps.map((entry) => entry.durationMs)),
            entries: frameGaps
          }
        }
      }
    }
  })
}

export async function startCoreFlowPerformanceProbe(
  electronApp: ElectronApplication,
  page: Page
): Promise<CoreFlowPerformanceProbe> {
  const startedAt = nodePerformance.now()
  const steps: StepMeasurement[] = []
  const processSamples: ProcessSample[] = []
  let sampling = true

  await startRendererProbe(page)
  const profileSession =
    process.env.MCODE_CORE_FLOW_PROFILE === '1' ? await page.context().newCDPSession(page) : null
  if (profileSession) {
    await profileSession.send('Profiler.enable')
    await profileSession.send('Profiler.start')
  }
  const samplingLoop = (async () => {
    while (sampling) {
      try {
        processSamples.push(await readProcessSample(electronApp, startedAt))
      } catch {
        // Main-world CDP contexts can be replaced briefly during app navigation.
      }
      await new Promise((resolve) => setTimeout(resolve, 100))
    }
  })()

  return {
    async measure<T>(label: string, action: () => Promise<T>): Promise<T> {
      const stepStartedAt = nodePerformance.now()
      try {
        return await action()
      } finally {
        steps.push({
          label,
          startMs: round(stepStartedAt - startedAt),
          durationMs: round(nodePerformance.now() - stepStartedAt)
        })
      }
    },
    async stop(testInfo: TestInfo): Promise<void> {
      sampling = false
      await samplingLoop.catch(() => undefined)
      const cpuProfile = profileSession
        ? await profileSession.send('Profiler.stop').catch(() => null)
        : null
      await profileSession?.detach().catch(() => undefined)
      const renderer = await page
        .evaluate(
          () =>
            (
              window as typeof window & {
                __mcodeCoreFlowPerformance?: { stop: () => RendererResponsiveness }
              }
            ).__mcodeCoreFlowPerformance?.stop() ?? null
        )
        .catch(() => null)
      const totalMs = round(nodePerformance.now() - startedAt)
      const report = {
        benchmark: 'existing-user-golden-core-flow',
        capturedAt: new Date().toISOString(),
        totalMs,
        steps,
        process: {
          sampleCount: processSamples.length,
          cpuPercent: summarize(processSamples.map((sample) => sample.cpuPercent)),
          workingSetMb: summarize(processSamples.map((sample) => sample.workingSetMb)),
          privateMb: summarize(processSamples.map((sample) => sample.privateMb)),
          peakProcessCount: Math.max(0, ...processSamples.map((sample) => sample.processCount)),
          samples: processSamples
        },
        renderer: renderer as RendererResponsiveness | null
      }
      await testInfo.attach('core-flow-performance', {
        body: JSON.stringify(report, null, 2),
        contentType: 'application/json'
      })
      const outputDir = process.env.MCODE_CORE_FLOW_OUTPUT_DIR
      if (outputDir) {
        const resolvedOutputDir = path.resolve(outputDir)
        mkdirSync(resolvedOutputDir, { recursive: true })
        const stem = `core-flow-${testInfo.repeatEachIndex}-${Date.now()}`
        writeFileSync(
          path.join(resolvedOutputDir, `${stem}.json`),
          `${JSON.stringify(report, null, 2)}\n`
        )
        if (cpuProfile) {
          writeFileSync(
            path.join(resolvedOutputDir, `${stem}.cpuprofile`),
            JSON.stringify(cpuProfile.profile)
          )
        }
      }
      if (cpuProfile) {
        await testInfo.attach('core-flow-renderer-cpu-profile', {
          body: JSON.stringify(cpuProfile.profile),
          contentType: 'application/json'
        })
      }
      console.log(
        `[core-flow-performance] ${JSON.stringify({
          ...report,
          process: { ...report.process, samples: undefined },
          renderer: renderer
            ? {
                ...renderer,
                longTasks: { ...renderer.longTasks, entries: undefined },
                frameGaps: { ...renderer.frameGaps, entries: undefined }
              }
            : null
        })}`
      )
    }
  }
}
