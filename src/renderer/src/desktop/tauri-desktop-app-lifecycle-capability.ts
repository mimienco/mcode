import type {
  DesktopAppLifecycleCapability,
  DesktopInvoke,
  ShutdownIntent,
  ShutdownRequest
} from './desktop-client'
import { isShutdownRequest } from './desktop-client'

type TauriEvent<T> = {
  payload: T
}

export type TauriListen = (
  event: string,
  listener: (event: TauriEvent<unknown>) => void
) => Promise<() => void>

export function createTauriDesktopAppLifecycleCapability(
  invoke: DesktopInvoke,
  listen: TauriListen
): DesktopAppLifecycleCapability {
  return {
    async requestShutdown(intent: ShutdownIntent): Promise<ShutdownRequest> {
      const response = await invoke('app_request_shutdown', { intent })
      if (!isShutdownRequest(response)) {
        throw new Error('Invalid app_request_shutdown response')
      }
      return response
    },
    onShutdownRequested(
      listener: (request: ShutdownRequest) => void | Promise<void>
    ): Promise<() => void> {
      return listen('mcode://app/shutdown-requested', (event) => {
        if (!isShutdownRequest(event.payload)) {
          console.error('[desktop] Ignored invalid native shutdown request')
          return
        }
        const request = event.payload
        void invoke('app_acknowledge_shutdown', { requestId: request.requestId })
          .then(() => listener(request))
          .catch(async (error) => {
            try {
              await invoke('app_cancel_shutdown', { requestId: request.requestId })
            } catch (cancelError) {
              console.error(
                '[desktop] Failed to cancel rejected native shutdown request',
                cancelError
              )
            }
            console.error('[desktop] Native shutdown request listener failed', error)
          })
      })
    },
    async confirmShutdown(requestId: string): Promise<ShutdownRequest> {
      const response = await invoke('app_confirm_shutdown', { requestId })
      if (!isShutdownRequest(response)) {
        throw new Error('Invalid app_confirm_shutdown response')
      }
      return response
    },
    async executeShutdown(requestId: string): Promise<void> {
      await invoke('app_execute_shutdown', { requestId })
    },
    async cancelShutdown(requestId: string): Promise<void> {
      await invoke('app_cancel_shutdown', { requestId })
    }
  }
}
