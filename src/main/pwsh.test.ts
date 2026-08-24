import { beforeEach, describe, expect, it, vi } from 'vitest'

const { execFileMock, execFileSyncMock, resolvePwshExecutableMock } = vi.hoisted(() => ({
  execFileMock: vi.fn(),
  execFileSyncMock: vi.fn(),
  resolvePwshExecutableMock: vi.fn()
}))

vi.mock('node:child_process', () => ({
  execFile: execFileMock,
  execFileSync: execFileSyncMock
}))

vi.mock('./providers/windows-powershell-executable', () => ({
  resolveWindowsPowerShellExecutablePath: resolvePwshExecutableMock
}))

function setPlatform(platform: NodeJS.Platform): () => void {
  const originalPlatform = process.platform
  Object.defineProperty(process, 'platform', { configurable: true, value: platform })
  return () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: originalPlatform })
  }
}

describe('PowerShell 7 availability', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.useRealTimers()
    execFileMock.mockReset()
    execFileSyncMock.mockReset()
    resolvePwshExecutableMock.mockReset().mockReturnValue('C:\\Program Files\\PowerShell\\7\\pwsh.exe')
  })

  it('returns false without probing on non-Windows platforms', async () => {
    const restorePlatform = setPlatform('linux')
    try {
      const { isPwshAvailable } = await import('./pwsh')
      expect(isPwshAvailable()).toBe(false)
      expect(resolvePwshExecutableMock).not.toHaveBeenCalled()
    } finally {
      restorePlatform()
    }
  })

  it('reports whether the ConPTY-safe PowerShell resolver finds pwsh.exe', async () => {
    const restorePlatform = setPlatform('win32')
    try {
      const { isPwshAvailable } = await import('./pwsh')
      expect(isPwshAvailable()).toBe(true)
      expect(resolvePwshExecutableMock).toHaveBeenCalledWith('pwsh.exe')
    } finally {
      restorePlatform()
    }
  })

  it('returns false when no launchable pwsh.exe exists', async () => {
    const restorePlatform = setPlatform('win32')
    resolvePwshExecutableMock.mockReturnValue(null)
    try {
      const { isPwshAvailable } = await import('./pwsh')
      expect(isPwshAvailable()).toBe(false)
    } finally {
      restorePlatform()
    }
  })

  it('reuses positive results for the process lifetime', async () => {
    const restorePlatform = setPlatform('win32')
    try {
      const { isPwshAvailable } = await import('./pwsh')
      expect(isPwshAvailable()).toBe(true)
      expect(isPwshAvailable()).toBe(true)
      expect(resolvePwshExecutableMock).toHaveBeenCalledTimes(1)
    } finally {
      restorePlatform()
    }
  })

  it('retries a negative result after the cache TTL', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(1_000)
    const restorePlatform = setPlatform('win32')
    resolvePwshExecutableMock.mockReturnValueOnce(null).mockReturnValue('C:\\pwsh.exe')
    try {
      const { isPwshAvailable } = await import('./pwsh')
      expect(isPwshAvailable()).toBe(false)
      expect(isPwshAvailable()).toBe(false)
      vi.setSystemTime(31_001)
      expect(isPwshAvailable()).toBe(true)
      expect(resolvePwshExecutableMock).toHaveBeenCalledTimes(2)
    } finally {
      restorePlatform()
      vi.useRealTimers()
    }
  })

  it('answers IPC callers without spawning a process', async () => {
    const restorePlatform = setPlatform('win32')
    try {
      const { isPwshAvailableAsync } = await import('./pwsh')
      await expect(
        Promise.all([isPwshAvailableAsync(), isPwshAvailableAsync()])
      ).resolves.toEqual([true, true])
      expect(resolvePwshExecutableMock).toHaveBeenCalledTimes(1)
      expect(execFileMock).not.toHaveBeenCalled()
      expect(execFileSyncMock).not.toHaveBeenCalled()
    } finally {
      restorePlatform()
    }
  })

  it('warms the same spawn-free cache used by synchronous callers', async () => {
    const restorePlatform = setPlatform('win32')
    try {
      const { isPwshAvailable, warmPwshAvailabilityCache } = await import('./pwsh')
      await expect(warmPwshAvailabilityCache()).resolves.toBe(true)
      expect(isPwshAvailable()).toBe(true)
      expect(resolvePwshExecutableMock).toHaveBeenCalledTimes(1)
      expect(execFileMock).not.toHaveBeenCalled()
      expect(execFileSyncMock).not.toHaveBeenCalled()
    } finally {
      restorePlatform()
    }
  })
})
