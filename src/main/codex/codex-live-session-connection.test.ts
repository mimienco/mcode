import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CodexLiveSessionCatalog } from './codex-live-session-catalog'
import { CodexLiveSessionConnection } from './codex-live-session-connection'

const catalog: CodexLiveSessionCatalog = {
  models: [
    {
      id: 'gpt-test',
      label: 'GPT Test',
      isDefault: true,
      defaultEffort: 'medium',
      efforts: []
    }
  ],
  permissionProfiles: ['ask', 'auto-edits', 'bypass'],
  collaborationModes: [
    { interactionMode: 'plan', wireMode: 'plan' },
    { interactionMode: 'build', wireMode: 'default' }
  ]
}

type MutableConnectionState = {
  threadId: string
  current: {
    model: string
    permission: 'ask'
    interactionMode: 'build'
  }
  subscribed: boolean
  settingsUpdateSupported: boolean
}

type FakeRpc = {
  available: boolean
  onNotification: ReturnType<typeof vi.fn>
  request: ReturnType<typeof vi.fn>
  dispose: ReturnType<typeof vi.fn>
}

function connectionWithoutEffectiveEcho(): CodexLiveSessionConnection {
  const rpc = {
    available: true,
    onNotification: vi.fn(),
    request: vi.fn().mockResolvedValue({}),
    dispose: vi.fn()
  }
  const Constructor = CodexLiveSessionConnection as unknown as new (
    rpc: FakeRpc,
    catalog: CodexLiveSessionCatalog
  ) => CodexLiveSessionConnection
  const connection = new Constructor(rpc, catalog)
  Object.assign(connection as unknown as MutableConnectionState, {
    threadId: 'thread-1',
    current: { model: 'gpt-test', permission: 'ask', interactionMode: 'build' },
    subscribed: true,
    settingsUpdateSupported: true
  })
  return connection
}

afterEach(() => {
  vi.useRealTimers()
})

describe('Codex live session update confirmation', () => {
  it('does not claim a changed setting applied without an effective-state echo', async () => {
    vi.useFakeTimers()
    const connection = connectionWithoutEffectiveEcho()

    const pending = connection.update({ interactionMode: 'plan' })
    await vi.advanceTimersByTimeAsync(751)

    await expect(pending).resolves.toEqual({ outcome: 'unavailable' })
  })

  it('accepts a provider-reported no-op without waiting for a notification', async () => {
    const connection = connectionWithoutEffectiveEcho()

    await expect(connection.update({ permission: 'ask' })).resolves.toEqual({
      outcome: 'applied',
      model: 'gpt-test',
      permission: 'ask',
      interactionMode: 'build'
    })
  })
})
