import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { createTauriFsClient } from './fs-client'
import { createTauriAgentStatusClient } from './tauri-agent-status-client'
import { createTauriAgentHooksClient } from './tauri-agent-hooks-client'
import type {
  AgentStatusClearIpcPayload,
  AgentStatusIpcPayload
} from '../../../shared/agent-status-types'

export async function runAgentStatusSmoke(parameters: URLSearchParams): Promise<void> {
  const marker = parameters.get('marker')
  const paneKey = parameters.get('paneKey')
  if (!marker || !paneKey) {
    throw new Error('agent_status smoke marker or pane key is missing')
  }
  const fs = createTauriFsClient(invoke, listen)
  const armed: Promise<unknown>[] = []
  const client = createTauriAgentStatusClient(invoke, (event, callback) => {
    const subscription = listen(event, callback)
    armed.push(subscription)
    return subscription
  })
  const hooks = createTauriAgentHooksClient(invoke)
  const set = Promise.withResolvers<AgentStatusIpcPayload>()
  const clear = Promise.withResolvers<AgentStatusClearIpcPayload>()
  const stopSet = client.onSet((payload) => {
    if (payload.paneKey === paneKey && payload.state === 'working') {
      set.resolve(payload)
    }
  })
  const stopClear = client.onClear((payload) => {
    if ('paneKey' in payload && payload.paneKey === paneKey) {
      clear.resolve(payload)
    }
  })
  const writeVerdict = (content: string): Promise<void> =>
    fs.writeFileAtomic({ filePath: marker, content })
  const bounded = async <T>(promise: Promise<T>, event: string): Promise<T> => {
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      return await Promise.race([
        promise,
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error(`${event} timed out`)), 15_000)
        })
      ])
    } finally {
      clearTimeout(timer)
    }
  }
  try {
    await Promise.all(armed)
    await writeVerdict('ready')
    const payload = await bounded(set.promise, 'agentStatus:set')
    if (payload.prompt !== 'wave37 smoke' || payload.agentType !== 'claude') {
      throw new Error('agentStatus:set did not preserve hook identity and prompt')
    }
    const inferred = await client.inferInterrupt({
      paneKey,
      baselineUpdatedAt: payload.receivedAt,
      baselineStateStartedAt: payload.stateStartedAt,
      baselinePrompt: payload.prompt,
      baselineAgentType: payload.agentType,
      intent: 'plain-escape'
    })
    if (!inferred) {
      throw new Error('inferInterrupt did not apply')
    }
    const interrupted = (await client.getSnapshot()).find((row) => row.paneKey === paneKey)
    if (interrupted?.state !== 'done' || interrupted.interrupted !== true) {
      throw new Error('inferred interruption was not visible in the snapshot')
    }
    await client.drop(paneKey)
    await bounded(clear.promise, 'agentStatus:clear')
    if (
      (await client.getSnapshot()).some(
        (row) => row.paneKey === paneKey && !row.providerSessionOnly
      )
    ) {
      throw new Error('drop did not clear the smoke pane')
    }
    if ((await hooks.codexStatus()).state !== 'skipped') {
      throw new Error('unported hook installer did not report skipped')
    }
    await writeVerdict('passed')
    document.getElementById('status')?.replaceChildren('agentStatus smoke passed')
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    await writeVerdict(`renderer-failed: ${message}`)
    throw error
  } finally {
    stopSet()
    stopClear()
  }
}
