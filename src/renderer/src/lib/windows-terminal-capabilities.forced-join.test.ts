// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  loadWindowsTerminalCapabilities,
  resetWindowsTerminalCapabilitiesForTests
} from './windows-terminal-capabilities'

// PERF-10: a forced load must join an identical in-flight probe instead of
// launching a duplicate wsl.exe/pwsh.exe probe, then revalidate at most once.
function stubProbeApi(wslIsAvailable: ReturnType<typeof vi.fn>): void {
  vi.stubGlobal('window', {
    api: {
      wsl: { isAvailable: wslIsAvailable, listDistros: vi.fn().mockResolvedValue([]) },
      pwsh: { isAvailable: vi.fn().mockResolvedValue(true) },
      gitBash: { isAvailable: vi.fn().mockResolvedValue(false) },
      runtime: { getStatus: vi.fn().mockResolvedValue({ hostPlatform: 'win32' }) }
    }
  })
}

describe('forced capability loads share the in-flight probe', () => {
  afterEach(() => {
    resetWindowsTerminalCapabilitiesForTests()
    vi.unstubAllGlobals()
  })

  it('joins an in-flight probe when forced and schedules one follow-up revalidation', async () => {
    const wslIsAvailable = vi.fn().mockResolvedValue(true)
    stubProbeApi(wslIsAvailable)
    // Why: deferred probes make probe concurrency directly observable.
    let releaseFirst!: (value: boolean) => void
    wslIsAvailable.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          releaseFirst = resolve
        })
    )

    const joined = loadWindowsTerminalCapabilities({ ownerKey: 'local' })
    const forced = loadWindowsTerminalCapabilities({ ownerKey: 'local', force: true })

    expect(wslIsAvailable).toHaveBeenCalledTimes(1)
    releaseFirst(true)
    await Promise.all([joined, forced])

    // The forced generation revalidates exactly once after the joined read.
    await vi.waitFor(() => expect(wslIsAvailable).toHaveBeenCalledTimes(2))
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(wslIsAvailable).toHaveBeenCalledTimes(2)
  })

  it('keeps unforced concurrent loads on a single shared probe', async () => {
    const wslIsAvailable = vi.fn().mockResolvedValue(true)
    stubProbeApi(wslIsAvailable)

    await Promise.all([
      loadWindowsTerminalCapabilities({ ownerKey: 'local' }),
      loadWindowsTerminalCapabilities({ ownerKey: 'local' })
    ])
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(wslIsAvailable).toHaveBeenCalledTimes(1)
  })
})
