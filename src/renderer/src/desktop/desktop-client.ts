import type { ShutdownCheckpoint } from '../../../shared/shutdown-checkpoint-types'

export type BuildLane = 'development' | 'production'

export type LaunchContext = {
  version: string
  buildLane: BuildLane
  headless: boolean
  safeMode: boolean
}

export type DesktopInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

export type ShutdownIntent = 'quit' | 'restart'

export type ShutdownRequest = {
  version: 1
  requestId: string
  intent: ShutdownIntent
}

export type DesktopAppLifecycleCapability = {
  requestShutdown: (intent: ShutdownIntent) => Promise<ShutdownRequest>
  onShutdownRequested: (
    listener: (request: ShutdownRequest) => void | Promise<void>
  ) => Promise<() => void>
  cancelShutdown: (requestId: string) => Promise<void>
  confirmShutdown: (requestId: string) => Promise<ShutdownRequest>
  executeShutdown: (requestId: string) => Promise<void>
}

export function isShutdownRequest(value: unknown): value is ShutdownRequest {
  if (!value || typeof value !== 'object') {
    return false
  }
  const request = value as Record<string, unknown>
  return (
    request.version === 1 &&
    typeof request.requestId === 'string' &&
    /^[1-9]\d*$/.test(request.requestId) &&
    (request.intent === 'quit' || request.intent === 'restart')
  )
}

export type DesktopWindowCloseRequest = {
  isQuitting: boolean
}

export type DesktopWindowCapability = {
  minimize: () => Promise<void>
  toggleMaximize: () => Promise<void>
  isMaximized: () => Promise<boolean>
  onMaximizedChanged: (listener: (maximized: boolean) => void) => Promise<() => void>
  requestClose: () => Promise<void>
  onCloseRequested: (
    listener: (request: DesktopWindowCloseRequest) => void | Promise<void>
  ) => Promise<() => void>
  cancelClose: () => Promise<ShutdownRequest | undefined>
  confirmClose: () => Promise<void>
}

function isLaunchContext(value: unknown): value is LaunchContext {
  if (!value || typeof value !== 'object') {
    return false
  }
  const context = value as Record<string, unknown>
  return (
    typeof context.version === 'string' &&
    (context.buildLane === 'development' || context.buildLane === 'production') &&
    typeof context.headless === 'boolean' &&
    typeof context.safeMode === 'boolean'
  )
}

export function createDesktopClient(
  invoke: DesktopInvoke,
  windowCapability: DesktopWindowCapability,
  appLifecycleCapability: DesktopAppLifecycleCapability
) {
  return {
    app: {
      async getLaunchContext(): Promise<LaunchContext> {
        const response = await invoke('app_get_launch_context')
        if (!isLaunchContext(response)) {
          throw new Error('Invalid app_get_launch_context response')
        }
        return response
      },
      async stageShutdownCheckpoint(checkpoint: ShutdownCheckpoint): Promise<void> {
        await invoke('app_stage_shutdown_checkpoint', { checkpoint })
      },
      requestShutdown: appLifecycleCapability.requestShutdown,
      onShutdownRequested: appLifecycleCapability.onShutdownRequested,
      cancelShutdown: appLifecycleCapability.cancelShutdown,
      confirmShutdown: appLifecycleCapability.confirmShutdown,
      executeShutdown: appLifecycleCapability.executeShutdown
    },
    window: windowCapability
  }
}

export type DesktopClient = ReturnType<typeof createDesktopClient>
