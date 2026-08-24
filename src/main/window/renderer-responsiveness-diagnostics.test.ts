import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { recordDurableCrashBreadcrumb, getAppMetrics } = vi.hoisted(() => ({
  recordDurableCrashBreadcrumb: vi.fn(),
  getAppMetrics: vi.fn(() => [
    {
      pid: 4242,
      cpu: { percentCPUUsage: 87.26 },
      memory: { workingSetSize: 128 * 1024, privateBytes: 96 * 1024 }
    }
  ])
}))

vi.mock('electron', () => ({ app: { getAppMetrics } }))

vi.mock('../crash-reporting/durable-crash-breadcrumb', () => ({
  recordDurableCrashBreadcrumb
}))

import { installRendererResponsivenessDiagnostics } from './renderer-responsiveness-diagnostics'

describe('renderer responsiveness diagnostics', () => {
  beforeEach(() => {
    recordDurableCrashBreadcrumb.mockReset()
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('records one native unresponsive episode and its recovery duration', () => {
    const handlers = new Map<string, () => void>()
    const target = {
      on: vi.fn((event: string, handler: () => void) => handlers.set(event, handler)),
      webContents: { id: 17, getOSProcessId: () => 4242 }
    }
    let now = 10_000
    installRendererResponsivenessDiagnostics(target as never, { now: () => now })

    handlers.get('unresponsive')?.()
    handlers.get('unresponsive')?.()
    now = 13_250
    handlers.get('responsive')?.()

    expect(recordDurableCrashBreadcrumb.mock.calls).toEqual([
      [
        'renderer_unresponsive',
        {
          episode: 1,
          webContentsId: 17,
          rendererPid: 4242,
          cpuPercent: 87.3,
          workingSetMB: 128,
          privateMemoryMB: 96
        }
      ],
      [
        'renderer_responsive',
        {
          episode: 1,
          durationMs: 3_250,
          webContentsId: 17,
          rendererPid: 4242,
          cpuPercent: 87.3,
          workingSetMB: 128,
          privateMemoryMB: 96
        }
      ]
    ])
  })

  it('ignores a responsive event without a matching stall', () => {
    const handlers = new Map<string, () => void>()
    const target = {
      on: vi.fn((event: string, handler: () => void) => handlers.set(event, handler)),
      webContents: { id: 18, getOSProcessId: () => 4343 }
    }
    installRendererResponsivenessDiagnostics(target as never)

    handlers.get('responsive')?.()

    expect(recordDurableCrashBreadcrumb).not.toHaveBeenCalled()
  })

  it('keeps diagnostics best-effort when the durable recorder fails', () => {
    const handlers = new Map<string, () => void>()
    const target = {
      on: vi.fn((event: string, handler: () => void) => handlers.set(event, handler)),
      webContents: { id: 19, getOSProcessId: () => 4242 }
    }
    const record = vi.fn(() => {
      throw new Error('trace sink unavailable')
    })
    let now = 20_000
    installRendererResponsivenessDiagnostics(target as never, { now: () => now, record })

    expect(() => handlers.get('unresponsive')?.()).not.toThrow()
    now = 21_500
    expect(() => handlers.get('responsive')?.()).not.toThrow()

    expect(record).toHaveBeenCalledTimes(2)
    expect(console.error).toHaveBeenCalledTimes(2)
  })
})
