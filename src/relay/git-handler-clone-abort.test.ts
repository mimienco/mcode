/**
 * Regression test: cancelling a relay git clone must kill the whole process
 * tree, not just the direct git child — on Windows SSH/credential-helper
 * descendants otherwise outlive the client disconnect (BH-RUNTIME-004).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { EventEmitter } from 'node:events'

const { terminateSpy, spawnMock } = vi.hoisted(() => ({
  terminateSpy: vi.fn(),
  spawnMock: vi.fn()
}))
vi.mock('./subprocess-tree-termination', () => ({
  terminateRelaySubprocessTree: (...args: unknown[]) => terminateSpy(...args)
}))
vi.mock('node:child_process', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, spawn: spawnMock }
})

import { GitHandler } from './git-handler'
import { RelayContext } from './context'
import { createMockDispatcher, type MockDispatcher } from './git-handler-test-setup'

function fakeGitChild(): EventEmitter & { pid: number; kill: ReturnType<typeof vi.fn> } {
  const child = new EventEmitter() as EventEmitter & {
    pid: number
    kill: ReturnType<typeof vi.fn>
  }
  child.pid = 4242
  child.kill = vi.fn()
  // spawnClone attaches data listeners; give them inert emitters.
  ;(child as unknown as Record<string, unknown>).stdout = new EventEmitter()
  ;(child as unknown as Record<string, unknown>).stderr = new EventEmitter()
  return child
}

describe('git.clone cancellation', () => {
  let dispatcher: MockDispatcher

  beforeEach(() => {
    terminateSpy.mockClear()
    spawnMock.mockReset()
    dispatcher = createMockDispatcher()
    void new GitHandler(dispatcher as never, new RelayContext())
  })

  it('terminates the subprocess tree when the request is aborted mid-clone', async () => {
    const child = fakeGitChild()
    spawnMock.mockReturnValue(child)

    const controller = new AbortController()
    const clone = dispatcher.callRequest(
      'git.clone',
      {
        args: ['clone', '--', 'https://example.com/repo.git', 'dest'],
        cwd: '/tmp',
        progressId: 'p1'
      },
      { isStale: () => false, signal: controller.signal }
    ) as Promise<unknown>

    await vi.waitFor(() => expect(spawnMock).toHaveBeenCalledTimes(1))
    controller.abort()

    expect(terminateSpy).toHaveBeenCalledWith(child)
    expect(child.kill).not.toHaveBeenCalled()

    // Settle the promise so the runner has no dangling rejection.
    child.emit('close', null, 'SIGKILL')
    await expect(clone).rejects.toThrow('Clone aborted')
  })
})
