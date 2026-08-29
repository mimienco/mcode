import type { ShutdownCheckpoint } from '../../../shared/shutdown-checkpoint-types'

export type BuildLane = 'development' | 'production'

export type LaunchContext = {
  version: string
  buildLane: BuildLane
  headless: boolean
  safeMode: boolean
}

export type DesktopInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

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
  windowCapability: DesktopWindowCapability
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
      }
    },
    window: windowCapability
  }
}

export type DesktopClient = ReturnType<typeof createDesktopClient>
