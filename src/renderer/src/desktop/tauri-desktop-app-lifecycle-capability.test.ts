import { expect, it, vi } from 'vitest'
import { createTauriDesktopAppLifecycleCapability } from './tauri-desktop-app-lifecycle-capability'

it('requests a restart through the narrow versioned lifecycle command', async () => {
  const invoke = vi.fn(async () => ({
    version: 1,
    requestId: '7',
    intent: 'restart'
  }))
  const capability = createTauriDesktopAppLifecycleCapability(invoke, vi.fn())

  const request = await capability.requestShutdown('restart')

  expect(invoke).toHaveBeenCalledWith('app_request_shutdown', { intent: 'restart' })
  expect(request).toEqual({ version: 1, requestId: '7', intent: 'restart' })
})

it('subscribes to versioned native shutdown requests through one named event', async () => {
  let eventListener: ((event: { payload: unknown }) => void) | undefined
  const listen = vi.fn(async (_event: string, listener: (event: { payload: unknown }) => void) => {
    eventListener = listener
    return () => undefined
  })
  const listener = vi.fn()
  const capability = createTauriDesktopAppLifecycleCapability(vi.fn(), listen)

  await capability.onShutdownRequested(listener)
  eventListener?.({ payload: { version: 1, requestId: '8', intent: 'quit' } })

  expect(listen).toHaveBeenCalledWith('mcode://app/shutdown-requested', expect.any(Function))
  expect(listener).toHaveBeenCalledWith({ version: 1, requestId: '8', intent: 'quit' })
})

it('acknowledges, executes, and cancels only the named shutdown request', async () => {
  const request = { version: 1 as const, requestId: '9', intent: 'restart' as const }
  const invoke = vi.fn(async (command: string) =>
    command === 'app_confirm_shutdown' ? request : undefined
  )
  const capability = createTauriDesktopAppLifecycleCapability(invoke, vi.fn())

  await expect(capability.confirmShutdown('9')).resolves.toEqual(request)
  await capability.executeShutdown('9')
  await capability.cancelShutdown('9')

  expect(invoke.mock.calls).toEqual([
    ['app_confirm_shutdown', { requestId: '9' }],
    ['app_execute_shutdown', { requestId: '9' }],
    ['app_cancel_shutdown', { requestId: '9' }]
  ])
})

it('rejects malformed command responses before they reach lifecycle state', async () => {
  const capability = createTauriDesktopAppLifecycleCapability(
    vi.fn(async () => ({ version: 1, requestId: 9, intent: 'restart' })),
    vi.fn()
  )

  await expect(capability.requestShutdown('restart')).rejects.toThrow(
    'Invalid app_request_shutdown response'
  )
})

it('drops malformed native event payloads', async () => {
  let eventListener: ((event: { payload: unknown }) => void) | undefined
  const listener = vi.fn()
  const capability = createTauriDesktopAppLifecycleCapability(vi.fn(), async (_event, handler) => {
    eventListener = handler
    return () => undefined
  })
  const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

  await capability.onShutdownRequested(listener)
  eventListener?.({ payload: { version: 2, requestId: '9', intent: 'restart' } })

  expect(listener).not.toHaveBeenCalled()
  expect(consoleError).toHaveBeenCalledOnce()
  consoleError.mockRestore()
})

it('reports rejected shutdown listeners instead of leaking an unhandled rejection', async () => {
  let eventListener: ((event: { payload: unknown }) => void) | undefined
  const failure = new Error('close coordinator failed')
  const capability = createTauriDesktopAppLifecycleCapability(vi.fn(), async (_event, handler) => {
    eventListener = handler
    return () => undefined
  })
  const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

  await capability.onShutdownRequested(() => Promise.reject(failure))
  eventListener?.({ payload: { version: 1, requestId: '10', intent: 'quit' } })
  await vi.waitFor(() => {
    expect(consoleError).toHaveBeenCalledWith(
      '[desktop] Native shutdown request listener failed',
      failure
    )
  })

  consoleError.mockRestore()
})
