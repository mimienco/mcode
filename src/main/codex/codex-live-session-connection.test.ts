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
    effort?: string
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

  it('waits for the settings subscription before applying a changed setting', async () => {
    const catalogWithAlternative: CodexLiveSessionCatalog = {
      ...catalog,
      models: [
        ...catalog.models,
        {
          id: 'gpt-alternative',
          label: 'GPT Alternative',
          defaultEffort: 'medium',
          efforts: []
        }
      ]
    }
    let notify: (method: string, params: unknown) => void = () => undefined
    const rpc: FakeRpc = {
      available: true,
      onNotification: vi.fn((listener) => {
        notify = listener
      }),
      request: vi.fn(async (method: string) => {
        if (method === 'thread/resume') {
          return {
            model: 'gpt-test',
            reasoningEffort: 'medium',
            collaborationMode: { mode: 'default' }
          }
        }
        if (method === 'thread/settings/update') {
          notify('thread/settings/updated', {
            threadId: 'thread-1',
            threadSettings: {
              model: 'gpt-alternative',
              effort: 'medium'
            }
          })
        }
        return {}
      }),
      dispose: vi.fn()
    }
    const Constructor = CodexLiveSessionConnection as unknown as new (
      rpc: FakeRpc,
      catalog: CodexLiveSessionCatalog
    ) => CodexLiveSessionConnection
    const connection = new Constructor(rpc, catalogWithAlternative)
    Object.assign(connection as unknown as MutableConnectionState, {
      threadId: 'thread-1',
      current: { model: 'gpt-test', permission: 'ask', interactionMode: 'build' },
      subscribed: false,
      settingsUpdateSupported: true
    })

    await expect(connection.update({ model: 'gpt-alternative' })).resolves.toEqual({
      outcome: 'applied',
      model: 'gpt-alternative',
      effort: 'medium'
    })
    expect(rpc.request.mock.calls.map(([method]) => method)).toEqual([
      'thread/resume',
      'thread/settings/update'
    ])
  })

  it('accepts a successful pre-turn update when no rollout exists to subscribe to', async () => {
    const rpc: FakeRpc = {
      available: true,
      onNotification: vi.fn(),
      request: vi.fn(async (method: string) => {
        if (method === 'thread/resume') {
          throw new Error('no rollout found for thread id thread-1')
        }
        return {}
      }),
      dispose: vi.fn()
    }
    const Constructor = CodexLiveSessionConnection as unknown as new (
      rpc: FakeRpc,
      catalog: CodexLiveSessionCatalog
    ) => CodexLiveSessionConnection
    const connection = new Constructor(rpc, catalog)
    Object.assign(connection as unknown as MutableConnectionState, {
      threadId: 'thread-1',
      current: {
        model: 'gpt-test',
        effort: 'medium',
        permission: 'ask',
        interactionMode: 'build'
      },
      subscribed: false,
      settingsUpdateSupported: true
    })

    await expect(connection.update({ effort: 'low' })).resolves.toEqual({
      outcome: 'applied',
      model: 'gpt-test',
      effort: 'low',
      permission: 'ask',
      interactionMode: 'build'
    })
    expect(rpc.request).toHaveBeenCalledWith('thread/settings/update', {
      threadId: 'thread-1',
      effort: 'low'
    })
    expect(connection.snapshot()).toMatchObject({
      current: {
        model: 'gpt-test',
        effort: 'low',
        permission: 'ask',
        interactionMode: 'build'
      }
    })
  })
})

describe('Codex live session pre-thread snapshot', () => {
  function freshConnection(rpc: FakeRpc): CodexLiveSessionConnection {
    const Constructor = CodexLiveSessionConnection as unknown as new (
      rpc: FakeRpc,
      catalog: CodexLiveSessionCatalog
    ) => CodexLiveSessionConnection
    return new Constructor(rpc, catalog)
  }

  it('reports the catalog default model/effort before any thread exists', async () => {
    const rpc: FakeRpc = {
      available: true,
      onNotification: vi.fn(),
      request: vi.fn().mockResolvedValue({}),
      dispose: vi.fn()
    }
    const connection = freshConnection(rpc)

    await expect(connection.readySnapshot()).resolves.toEqual({
      available: true,
      models: [catalog.models[0]],
      controls: {
        permissions: ['ask', 'auto-edits', 'bypass'],
        interactionModes: ['plan', 'build']
      },
      current: { model: 'gpt-test', effort: 'medium' }
    })
    // The settings-support probe needs a thread; a pre-thread poll must not
    // trigger it (a premature false is cached permanently).
    expect(rpc.request).not.toHaveBeenCalledWith('thread/settings/update', expect.anything())
  })

  it('prefers the live-reported current over the catalog default', () => {
    const connection = connectionWithoutEffectiveEcho()

    expect(connection.snapshot()).toMatchObject({
      available: true,
      current: { model: 'gpt-test', permission: 'ask', interactionMode: 'build' }
    })
  })
})
