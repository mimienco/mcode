import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { summarizeTrace } from './apphang-report-summary.mjs'

const tempDirs = []

afterEach(() => {
  for (const dir of tempDirs.splice(0)) {
    rmSync(dir, { recursive: true, force: true })
  }
})

function traceEntry(name, data) {
  return JSON.stringify({
    type: 'effect-span',
    name: name.startsWith('renderer_') ? 'renderer.breadcrumb' : 'crash.breadcrumb',
    durationMs: 0,
    attributes: {
      kind: 'crash-breadcrumb',
      'breadcrumb.name': name,
      'breadcrumb.data': data
    }
  })
}

describe('Windows AppHang trace summary', () => {
  it('summarizes renderer and main stall episodes', () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'apphang-summary-'))
    tempDirs.push(dir)
    const logsDir = path.join(dir, 'logs')
    mkdirSync(logsDir)
    writeFileSync(
      path.join(logsDir, 'main.trace.ndjson'),
      [
        traceEntry('renderer_event_loop_stall', { blockedMs: 1_750 }),
        traceEntry('renderer_unresponsive', { cpuPercent: 92.1 }),
        traceEntry('renderer_responsive', { durationMs: 4_250 }),
        traceEntry('main_thread_unresponsive', { unresponsiveMs: 5_500 })
      ].join('\n')
    )

    expect(summarizeTrace(dir).stallSummary).toMatchObject({
      count: 4,
      byName: {
        renderer_event_loop_stall: 1,
        renderer_unresponsive: 1,
        renderer_responsive: 1,
        main_thread_unresponsive: 1
      },
      maxRendererBlockedMs: 1_750,
      maxNativeRendererUnresponsiveMs: 4_250,
      maxMainUnresponsiveMs: 5_500
    })
  })
})
