import { expect, it, vi } from 'vitest'
import type { DesktopAppLifecycleCapability } from './desktop-client'
import { runLifecycleSmoke } from './lifecycle-smoke-client'

function createLifecycle() {
  let listener: Parameters<DesktopAppLifecycleCapability['onShutdownRequested']>[0] | undefined
  const calls: string[] = []
  const lifecycle = {
    requestShutdown: vi.fn(async (intent: 'quit' | 'restart') => ({
      version: 1 as const,
      requestId: '21',
      intent
    })),
    onShutdownRequested: vi.fn(
      async (handler: Parameters<DesktopAppLifecycleCapability['onShutdownRequested']>[0]) => {
        listener = handler
        return () => undefined
      }
    ),
    cancelShutdown: vi.fn(() => Promise.resolve()),
    confirmShutdown: vi.fn(async (requestId: string) => {
      calls.push(`confirm:${requestId}`)
      return { version: 1 as const, requestId, intent: 'quit' as const }
    }),
    executeShutdown: vi.fn(async (requestId: string) => {
      calls.push(`execute:${requestId}`)
    })
  }
  return {
    calls,
    lifecycle,
    emit: (intent: 'quit' | 'restart') => listener?.({ version: 1, requestId: '21', intent })
  }
}

it('stages the durable checkpoint before acknowledging and executing native quit', async () => {
  const { calls, lifecycle, emit } = createLifecycle()

  await runLifecycleSmoke({
    lifecycle,
    intent: 'quit',
    stageCheckpoint: async () => {
      calls.push('checkpoint')
    }
  })
  await emit('quit')

  expect(calls).toEqual(['checkpoint', 'confirm:21', 'execute:21'])
  expect(lifecycle.requestShutdown).not.toHaveBeenCalled()
})

it('requests restart only after its native listener is installed', async () => {
  const { lifecycle } = createLifecycle()

  await runLifecycleSmoke({
    lifecycle,
    intent: 'restart',
    stageCheckpoint: () => Promise.resolve()
  })

  expect(lifecycle.onShutdownRequested.mock.invocationCallOrder[0]).toBeLessThan(
    lifecycle.requestShutdown.mock.invocationCallOrder[0]
  )
  expect(lifecycle.requestShutdown).toHaveBeenCalledWith('restart')
})
