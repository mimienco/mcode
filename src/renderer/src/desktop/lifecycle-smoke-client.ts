import type { DesktopAppLifecycleCapability, ShutdownIntent } from './desktop-client'

export type LifecycleSmokeCheckpoint = {
  version: 1
  sessions: []
  ui: Record<string, never>
}

type LifecycleSmokeDependencies = {
  lifecycle: DesktopAppLifecycleCapability
  intent: ShutdownIntent
  stageCheckpoint: (checkpoint: LifecycleSmokeCheckpoint) => Promise<void>
}

export async function runLifecycleSmoke({
  lifecycle,
  intent,
  stageCheckpoint
}: LifecycleSmokeDependencies): Promise<void> {
  await lifecycle.onShutdownRequested(async (request) => {
    await stageCheckpoint({ version: 1, sessions: [], ui: {} })
    await lifecycle.confirmShutdown(request.requestId)
    await lifecycle.executeShutdown(request.requestId)
  })

  if (intent === 'restart') {
    await lifecycle.requestShutdown('restart')
  }
}
