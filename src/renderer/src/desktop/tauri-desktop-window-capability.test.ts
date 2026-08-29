import { expect, it, vi } from 'vitest'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'

function createLifecycleCapability() {
  return {
    requestShutdown: (intent: 'quit' | 'restart') =>
      Promise.resolve({ version: 1 as const, requestId: '1', intent }),
    onShutdownRequested: () => Promise.resolve(() => undefined),
    cancelShutdown: () => Promise.resolve(),
    confirmAndExecuteShutdown: () => Promise.resolve()
  }
}

it('prevents a native close request and forwards it to the typed close coordinator', async () => {
  let onCloseRequested:
    | ((event: { preventDefault: () => void }) => void | Promise<void>)
    | undefined
  const preventDefault = vi.fn()
  const listener = vi.fn()
  const capability = createTauriDesktopWindowCapability(
    {
      minimize: () => Promise.resolve(),
      toggleMaximize: () => Promise.resolve(),
      isMaximized: () => Promise.resolve(false),
      onResized: () => Promise.resolve(() => undefined),
      close: () => Promise.resolve(),
      onCloseRequested: (handler) => {
        onCloseRequested = handler
        return Promise.resolve(() => undefined)
      }
    },
    createLifecycleCapability()
  )

  await capability.onCloseRequested(listener)
  await onCloseRequested?.({ preventDefault })

  expect(preventDefault).toHaveBeenCalledOnce()
  expect(listener).toHaveBeenCalledWith({ isQuitting: false })
})

it('turns a confirmed manual close into an authorized app quit without re-entering the coordinator', async () => {
  let onCloseRequested: ((event: { preventDefault: () => void }) => void) | undefined
  let shutdownListener:
    | ((request: {
        version: 1
        requestId: string
        intent: 'quit' | 'restart'
      }) => void | Promise<void>)
    | undefined
  const preventDefault = vi.fn()
  const listener = vi.fn()
  const request = { version: 1 as const, requestId: '10', intent: 'quit' as const }
  const lifecycle = {
    requestShutdown: vi.fn(async () => {
      await shutdownListener?.(request)
      return request
    }),
    onShutdownRequested: vi.fn(async (handler) => {
      shutdownListener = handler
      return () => undefined
    }),
    cancelShutdown: vi.fn(() => Promise.resolve()),
    confirmAndExecuteShutdown: vi.fn(() => Promise.resolve())
  }
  const source = {
    minimize: () => Promise.resolve(),
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onResized: () => Promise.resolve(() => undefined),
    close: vi.fn(async () => onCloseRequested?.({ preventDefault })),
    onCloseRequested: (
      handler: (event: { preventDefault: () => void }) => void
    ): Promise<() => void> => {
      onCloseRequested = handler
      return Promise.resolve(() => undefined)
    }
  }
  const capability = createTauriDesktopWindowCapability(source, lifecycle)
  await capability.onCloseRequested(listener)

  await capability.confirmClose()

  expect(lifecycle.requestShutdown).toHaveBeenCalledWith('quit')
  expect(lifecycle.confirmAndExecuteShutdown).toHaveBeenCalledWith('10')
  expect(source.close).not.toHaveBeenCalled()
  expect(preventDefault).not.toHaveBeenCalled()
  expect(listener).not.toHaveBeenCalled()
})

it('routes an app-wide restart through the same close coordinator before native execution', async () => {
  let shutdownListener:
    | ((request: { version: 1; requestId: string; intent: 'quit' | 'restart' }) => void)
    | undefined
  const closeListener = vi.fn()
  const currentWindow = {
    minimize: () => Promise.resolve(),
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onResized: () => Promise.resolve(() => undefined),
    close: vi.fn(() => Promise.resolve()),
    onCloseRequested: () => Promise.resolve(() => undefined)
  }
  const lifecycle = {
    requestShutdown: vi.fn(),
    onShutdownRequested: vi.fn(async (listener) => {
      shutdownListener = listener
      return () => undefined
    }),
    confirmAndExecuteShutdown: vi.fn(() => Promise.resolve()),
    cancelShutdown: vi.fn(() => Promise.resolve())
  }
  const capability = createTauriDesktopWindowCapability(currentWindow, lifecycle)
  await capability.onCloseRequested(closeListener)

  shutdownListener?.({ version: 1, requestId: '11', intent: 'restart' })
  await capability.confirmClose()

  expect(closeListener).toHaveBeenCalledWith({ isQuitting: true })
  expect(lifecycle.confirmAndExecuteShutdown).toHaveBeenCalledWith('11')
  expect(currentWindow.close).not.toHaveBeenCalled()
})

it('cancels a vetoed app request before routing a later native window close', async () => {
  let shutdownListener:
    | ((request: { version: 1; requestId: string; intent: 'quit' | 'restart' }) => Promise<void>)
    | undefined
  let nativeCloseListener:
    | ((event: { preventDefault: () => void }) => void | Promise<void>)
    | undefined
  const closeListener = vi.fn()
  const lifecycle = {
    requestShutdown: vi.fn(),
    onShutdownRequested: vi.fn(async (listener) => {
      shutdownListener = listener
      return () => undefined
    }),
    cancelShutdown: vi.fn(() => Promise.resolve()),
    confirmAndExecuteShutdown: vi.fn()
  }
  const capability = createTauriDesktopWindowCapability(
    {
      minimize: () => Promise.resolve(),
      toggleMaximize: () => Promise.resolve(),
      isMaximized: () => Promise.resolve(false),
      onResized: () => Promise.resolve(() => undefined),
      close: () => Promise.resolve(),
      onCloseRequested: async (listener) => {
        nativeCloseListener = listener
        return () => undefined
      }
    },
    lifecycle
  )
  await capability.onCloseRequested(closeListener)

  await shutdownListener?.({ version: 1, requestId: '12', intent: 'quit' })
  await nativeCloseListener?.({ preventDefault: vi.fn() })

  expect(lifecycle.cancelShutdown).toHaveBeenCalledWith('12')
  expect(closeListener.mock.calls).toEqual([[{ isQuitting: true }], [{ isQuitting: false }]])
})

it('returns the canceled restart request so the coordinator can clear renderer restart state', async () => {
  let shutdownListener:
    | ((request: { version: 1; requestId: string; intent: 'quit' | 'restart' }) => Promise<void>)
    | undefined
  const lifecycle = {
    requestShutdown: vi.fn(),
    onShutdownRequested: vi.fn(async (listener) => {
      shutdownListener = listener
      return () => undefined
    }),
    cancelShutdown: vi.fn(() => Promise.resolve()),
    confirmAndExecuteShutdown: vi.fn()
  }
  const capability = createTauriDesktopWindowCapability(
    {
      minimize: () => Promise.resolve(),
      toggleMaximize: () => Promise.resolve(),
      isMaximized: () => Promise.resolve(false),
      onResized: () => Promise.resolve(() => undefined),
      close: () => Promise.resolve(),
      onCloseRequested: () => Promise.resolve(() => undefined)
    },
    lifecycle
  )
  await capability.onCloseRequested(vi.fn())
  const request = { version: 1 as const, requestId: '14', intent: 'restart' as const }
  await shutdownListener?.(request)

  await expect(capability.cancelClose()).resolves.toEqual(request)
  expect(lifecycle.cancelShutdown).toHaveBeenCalledWith('14')
})
