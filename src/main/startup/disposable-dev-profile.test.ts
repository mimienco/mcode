import { describe, expect, it, vi } from 'vitest'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { cleanupDisposableDevProfile } from '../../../config/scripts/disposable-dev-profile.mjs'

describe('disposable dev profile cleanup', () => {
  it('retries a temporarily locked profile until removal succeeds', async () => {
    const remove = vi
      .fn()
      .mockRejectedValueOnce(Object.assign(new Error('locked'), { code: 'EBUSY' }))
      .mockRejectedValueOnce(Object.assign(new Error('locked'), { code: 'EPERM' }))
      .mockResolvedValueOnce(undefined)
    const sleep = vi.fn().mockResolvedValue(undefined)

    await expect(
      cleanupDisposableDevProfile(join(tmpdir(), 'mcode-dev-test'), {
        remove,
        sleep,
        timeoutMs: 10_000,
        now: vi.fn().mockReturnValueOnce(0).mockReturnValueOnce(1).mockReturnValueOnce(2)
      })
    ).resolves.toBe(true)

    expect(remove).toHaveBeenCalledTimes(3)
    expect(sleep).toHaveBeenNthCalledWith(1, 100)
    expect(sleep).toHaveBeenNthCalledWith(2, 200)
  })

  it('stops retrying after the cleanup deadline', async () => {
    const remove = vi.fn().mockRejectedValue(Object.assign(new Error('locked'), { code: 'EBUSY' }))
    const warn = vi.fn()

    await expect(
      cleanupDisposableDevProfile(join(tmpdir(), 'mcode-dev-test'), {
        remove,
        sleep: vi.fn().mockResolvedValue(undefined),
        timeoutMs: 0,
        now: () => 0,
        warn
      })
    ).resolves.toBe(false)

    expect(remove).toHaveBeenCalledTimes(1)
    expect(warn).toHaveBeenCalledOnce()
  })
})
