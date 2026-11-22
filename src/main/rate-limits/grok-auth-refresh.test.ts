import { EventEmitter } from 'node:events'
import { afterEach, describe, expect, it, vi } from 'vitest'

const spawnMock = vi.hoisted(() => vi.fn())

vi.mock('node:child_process', () => ({
  spawn: spawnMock
}))

import { refreshGrokAuthWithCli } from './grok-auth-refresh'

function mockChild(): EventEmitter & { kill: ReturnType<typeof vi.fn> } {
  return Object.assign(new EventEmitter(), { kill: vi.fn() })
}

describe('refreshGrokAuthWithCli', () => {
  afterEach(() => {
    spawnMock.mockReset()
    vi.useRealTimers()
  })

  it('runs the headless model listing with no terminal or captured credential output', async () => {
    const child = mockChild()
    spawnMock.mockReturnValue(child)

    const refresh = refreshGrokAuthWithCli({ command: 'grok-test', cwd: '/grok-home' })
    child.emit('close', 0)
    await refresh

    expect(spawnMock).toHaveBeenCalledWith(
      'grok-test',
      ['models'],
      expect.objectContaining({ cwd: '/grok-home', stdio: 'ignore', windowsHide: true })
    )
  })

  it('fails without exposing child output when Grok exits unsuccessfully', async () => {
    const child = mockChild()
    spawnMock.mockReturnValue(child)

    const refresh = refreshGrokAuthWithCli({ command: 'grok-test', cwd: '/grok-home' })
    child.emit('close', 1)

    await expect(refresh).rejects.toThrow('Grok could not refresh its saved sign-in')
  })

  it('kills the refresh probe when its caller aborts', async () => {
    const child = mockChild()
    const controller = new AbortController()
    spawnMock.mockReturnValue(child)

    const refresh = refreshGrokAuthWithCli({
      command: 'grok-test',
      cwd: '/grok-home',
      signal: controller.signal
    })
    controller.abort()

    await expect(refresh).rejects.toThrow('Grok could not refresh its saved sign-in')
    expect(child.kill).toHaveBeenCalledOnce()
  })
})
