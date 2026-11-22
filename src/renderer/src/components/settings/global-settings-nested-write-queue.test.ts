import { describe, expect, it, vi } from 'vitest'
import { getDefaultSettings } from '../../../../shared/constants'
import type { GlobalSettings } from '../../../../shared/global-settings-types'
import { createGlobalSettingsNestedWriteQueue } from './global-settings-nested-write-queue'

type Deferred = {
  promise: Promise<void>
  resolve: () => void
}

function createDeferred(): Deferred {
  let resolve!: () => void
  const promise = new Promise<void>((next) => {
    resolve = next
  })
  return { promise, resolve }
}

async function flushPromiseQueue(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

// Mirrors the settings store: the persisted update merges only once the write's
// promise resolves (i.e. after the IPC round-trip reconciled).
function createDeferredStore(settings: GlobalSettings) {
  let latest = settings
  const writes: Deferred[] = []
  const updates: Partial<GlobalSettings>[] = []
  const updateSettings = vi.fn((update: Partial<GlobalSettings>) => {
    updates.push(update)
    const nextSettings = { ...latest, ...update }
    const write = createDeferred()
    writes.push(write)
    return write.promise.then(() => {
      latest = nextSettings
    })
  })
  return { updates, writes, updateSettings, getLatest: () => latest }
}

describe('createGlobalSettingsNestedWriteQueue', () => {
  it('lets a rapid second env-field commit observe the first reconciled write', async () => {
    const base = { ...getDefaultSettings('/tmp'), agentDefaultEnv: {} }
    const store = createDeferredStore(base)
    const queue = createGlobalSettingsNestedWriteQueue<GlobalSettings>()
    const options = {
      getSettings: store.getLatest,
      fallback: base,
      updateSettings: store.updateSettings
    }

    const firstWrite = queue(options)((latest) => ({
      agentDefaultEnv: { ...(latest.agentDefaultEnv ?? {}), CLAUDE_DEBUG: '1' }
    }))
    // Scheduled before the first persist resolved — a stale snapshot would drop CLAUDE_DEBUG.
    const secondWrite = queue(options)((latest) => ({
      agentDefaultEnv: { ...(latest.agentDefaultEnv ?? {}), CODEX_HOME: '/x' }
    }))

    await flushPromiseQueue()
    expect(store.updateSettings).toHaveBeenCalledTimes(1)
    expect(store.updates[0]).toMatchObject({ agentDefaultEnv: { CLAUDE_DEBUG: '1' } })

    store.writes[0].resolve()
    await firstWrite
    await flushPromiseQueue()

    expect(store.updateSettings).toHaveBeenCalledTimes(2)
    expect(store.updates[1]).toMatchObject({
      agentDefaultEnv: { CLAUDE_DEBUG: '1', CODEX_HOME: '/x' }
    })

    store.writes[1].resolve()
    await secondWrite
    expect(store.getLatest().agentDefaultEnv).toEqual({ CLAUDE_DEBUG: '1', CODEX_HOME: '/x' })
  })

  it('keeps repeated queued requests idempotent against the latest state', async () => {
    const settings: GlobalSettings = {
      ...getDefaultSettings('/tmp'),
      defaultTuiAgent: null,
      disabledTuiAgents: []
    }
    const store = createDeferredStore(settings)
    const queue = createGlobalSettingsNestedWriteQueue<GlobalSettings>()
    const options = {
      getSettings: store.getLatest,
      fallback: settings,
      updateSettings: store.updateSettings
    }

    const addClaude = (latest: GlobalSettings): Partial<GlobalSettings> => {
      const current = latest.disabledTuiAgents ?? []
      return { disabledTuiAgents: current.includes('claude') ? current : [...current, 'claude'] }
    }
    const firstWrite = queue(options)(addClaude)
    const secondWrite = queue(options)(addClaude)

    await flushPromiseQueue()
    store.writes[0].resolve()
    await firstWrite
    await flushPromiseQueue()

    expect(store.updateSettings).toHaveBeenCalledTimes(2)
    expect(store.updates[1]).toMatchObject({ disabledTuiAgents: ['claude'] })

    store.writes[1].resolve()
    await secondWrite
  })

  it('survives a rejected write without blocking the next one', async () => {
    const settings: GlobalSettings = { ...getDefaultSettings('/tmp') }
    let latest = settings
    let writeCount = 0
    const queue = createGlobalSettingsNestedWriteQueue<GlobalSettings>()
    const options = {
      getSettings: () => latest,
      fallback: settings,
      updateSettings: (update: Partial<GlobalSettings>) => {
        writeCount += 1
        if (writeCount === 1) {
          return Promise.reject(new Error('persist failed'))
        }
        latest = { ...latest, ...update }
        return Promise.resolve()
      }
    }

    await queue(options)(() => ({ defaultTuiAgent: 'blank' })).catch(() => undefined)
    await queue(options)((s) => ({ agentCmdOverrides: { ...(s.agentCmdOverrides ?? {}) } }))

    expect(writeCount).toBe(2)
    expect(latest.agentCmdOverrides).toEqual({})
  })
})
