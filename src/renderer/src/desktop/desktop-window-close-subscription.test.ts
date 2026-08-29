import { expect, it, vi } from 'vitest'
import { subscribeToDesktopWindowCloseRequests } from './desktop-window-close-subscription'

it('releases a close-request subscription that resolves after its owner unmounts', async () => {
  let resolveSubscription: ((unsubscribe: () => void) => void) | undefined
  const unsubscribe = vi.fn()
  const desktopWindow = {
    onCloseRequested: () =>
      new Promise<() => void>((resolve) => {
        resolveSubscription = resolve
      })
  }

  const cleanup = subscribeToDesktopWindowCloseRequests(desktopWindow, vi.fn())
  cleanup()
  resolveSubscription?.(unsubscribe)
  await Promise.resolve()

  expect(unsubscribe).toHaveBeenCalledOnce()
})

it('reports a rejected native subscription without creating an unhandled rejection', async () => {
  const failure = new Error('window listener unavailable')
  const onError = vi.fn()

  subscribeToDesktopWindowCloseRequests(
    { onCloseRequested: () => Promise.reject(failure) },
    vi.fn(),
    onError
  )
  await Promise.resolve()

  expect(onError).toHaveBeenCalledWith(failure)
})

it('reports a rejected async close handler while the prevented window stays open', async () => {
  let nativeListener: ((request: { isQuitting: boolean }) => void) | undefined
  const failure = new Error('checkpoint failed')
  const onError = vi.fn()
  subscribeToDesktopWindowCloseRequests(
    {
      onCloseRequested(listener) {
        nativeListener = listener
        return Promise.resolve(() => undefined)
      }
    },
    () => Promise.reject(failure),
    onError
  )
  await Promise.resolve()

  nativeListener?.({ isQuitting: false })
  await Promise.resolve()
  await Promise.resolve()

  expect(onError).toHaveBeenCalledWith(failure)
})
