import type { DesktopWindowCapability, DesktopWindowCloseRequest } from './desktop-client'

export function subscribeToDesktopWindowCloseRequests(
  desktopWindow: Pick<DesktopWindowCapability, 'onCloseRequested'>,
  listener: (request: DesktopWindowCloseRequest) => void | Promise<void>,
  onError: (error: unknown) => void = (error) => {
    console.error('[desktop] Failed to subscribe to native window close requests', error)
  }
): () => void {
  let active = true
  let unsubscribe: (() => void) | undefined
  const handleCloseRequest = (request: DesktopWindowCloseRequest): void => {
    try {
      void Promise.resolve(listener(request)).catch(onError)
    } catch (error) {
      onError(error)
    }
  }
  void desktopWindow.onCloseRequested(handleCloseRequest).then((stop) => {
    if (active) {
      unsubscribe = stop
    } else {
      stop()
    }
  }, onError)
  return () => {
    active = false
    unsubscribe?.()
  }
}
