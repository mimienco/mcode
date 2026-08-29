export type BuildLane = 'development' | 'production'

export type LaunchContext = {
  version: string
  buildLane: BuildLane
  headless: boolean
  safeMode: boolean
}

export type DesktopInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

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

export function createDesktopClient(invoke: DesktopInvoke) {
  return {
    app: {
      async getLaunchContext(): Promise<LaunchContext> {
        const response = await invoke('app_get_launch_context')
        if (!isLaunchContext(response)) {
          throw new Error('Invalid app_get_launch_context response')
        }
        return response
      }
    }
  }
}

export type DesktopClient = ReturnType<typeof createDesktopClient>
