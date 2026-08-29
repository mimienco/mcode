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

export type DesktopCommandErrorContext = {
  operation: string
  requestId?: string
  intent?: ShutdownIntent
}

export type DesktopCommandErrorPayload = {
  code: string
  context: DesktopCommandErrorContext
  message: string
  userMessage: string
}

export class DesktopCommandError extends Error {
  readonly name = 'DesktopCommandError'

  constructor(
    readonly code: string,
    readonly context: DesktopCommandErrorContext,
    message: string,
    readonly userMessage: string,
    readonly cause?: unknown
  ) {
    super(message)
  }
}

function isDesktopCommandErrorContext(value: unknown): value is DesktopCommandErrorContext {
  if (!value || typeof value !== 'object') {
    return false
  }
  const context = value as Record<string, unknown>
  return (
    typeof context.operation === 'string' &&
    (context.requestId === undefined || typeof context.requestId === 'string') &&
    (context.intent === undefined || context.intent === 'quit' || context.intent === 'restart')
  )
}

export function isDesktopCommandErrorPayload(value: unknown): value is DesktopCommandErrorPayload {
  if (!value || typeof value !== 'object') {
    return false
  }
  const error = value as Record<string, unknown>
  return (
    typeof error.code === 'string' &&
    isDesktopCommandErrorContext(error.context) &&
    typeof error.message === 'string' &&
    typeof error.userMessage === 'string'
  )
}

export function normalizeDesktopCommandError(
  value: unknown,
  operation: string
): DesktopCommandError {
  if (value instanceof DesktopCommandError) {
    return value
  }
  if (isDesktopCommandErrorPayload(value)) {
    return new DesktopCommandError(
      value.code,
      value.context,
      value.message,
      value.userMessage,
      value
    )
  }
  const message = value instanceof Error ? value.message : String(value)
  return new DesktopCommandError(
    'desktop_command_failed',
    { operation },
    message,
    'MCode could not complete the desktop request. Try again.',
    value
  )
}

export function createDesktopInvoke(invoke: DesktopInvoke): DesktopInvoke {
  return async (command, args) => {
    try {
      return await invoke(command, args)
    } catch (error) {
      throw normalizeDesktopCommandError(error, command)
    }
  }
}

export type DesktopAppLifecycleCapability = {
  requestShutdown: (intent: ShutdownIntent) => Promise<ShutdownRequest>
  onShutdownRequested: (
    listener: (request: ShutdownRequest) => void | Promise<void>
  ) => Promise<() => void>
  cancelShutdown: (requestId: string) => Promise<void>
  confirmAndExecuteShutdown: (requestId: string) => Promise<void>
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
      confirmAndExecuteShutdown: appLifecycleCapability.confirmAndExecuteShutdown
    },
    window: windowCapability
  }
}

export type DesktopClient = ReturnType<typeof createDesktopClient>
