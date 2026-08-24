import { EventEmitter } from 'node:events'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const spawnMock = vi.hoisted(() => vi.fn())

vi.mock('node:child_process', () => ({
  spawn: spawnMock
}))

import {
  _resetHydrateShellPathCache,
  hydrateShellPath
} from './hydrate-shell-path'

// PERF-15: a verbose profile must not balloon main-process memory. The probe
// kills the shell and reports output_overflow once captured stdout exceeds the
// hard byte bound.
describe('shell PATH probe output bound', () => {
  type FakeChild = {
    stdout: EventEmitter
    kill: (signal?: string) => boolean | undefined
  }
  function createFakeChild(): FakeChild & EventEmitter {
    const child = new EventEmitter() as FakeChild & EventEmitter
    child.stdout = new EventEmitter()
    child.kill = vi.fn()
    return child
  }

  beforeEach(() => {
    _resetHydrateShellPathCache()
    spawnMock.mockReset()
  })
  // Why: hydrateShellPath defers the spawn through a probe queue. The stdout
  // stream's 'newListener' event fires exactly when the probe attaches its
  // data handler — a deterministic attach signal, no wall-clock polling.
  function armAttachSignal(child: ReturnType<typeof createFakeChild>): Promise<void> {
    const attached = Promise.withResolvers<void>()
    child.stdout.once('newListener', () => attached.resolve())
    return attached.promise
  }

  it('reports output_overflow instead of buffering unbounded stdout', async () => {
    const child = createFakeChild()
    spawnMock.mockReturnValue(child)
    const attachSignal = armAttachSignal(child)
    const pending = hydrateShellPath({
      force: true,
      shellOverride: '/bin/bash'
    })

    // One chunk just over the 1 MiB capture bound.
    await attachSignal
    child.stdout.emit('data', Buffer.alloc(1024 * 1024 + 1))
    const result = await pending

    expect(result.ok).toBe(false)
    expect(result.failureReason).toBe('output_overflow')
    expect(child.kill).toHaveBeenCalledWith('SIGKILL')
  })
  it('finishes exactly once when the killed shell still emits close', async () => {
    const child = createFakeChild()
    spawnMock.mockReturnValue(child)
    const attachSignal = armAttachSignal(child)
    const promise = hydrateShellPath({ force: true, shellOverride: '/bin/bash' })
    await attachSignal
    child.stdout.emit('data', Buffer.alloc(1024 * 1024 + 1))
    child.emit('close', 137)
    const result = await promise

    expect(result.failureReason).toBe('output_overflow')
  })
})
