import { describe, expect, it, vi } from 'vitest'
import { createTauriSessionClient } from './tauri-session-client'
import { getDefaultWorkspaceSession } from '../../../shared/constants'

describe('Tauri session contract', () => {
  it('uses the existing preload methods and explicit host partitions', async () => {
    const local = getDefaultWorkspaceSession()
    const invoke = vi.fn(async () => local)
    const client = createTauriSessionClient(invoke)
    await expect(client.get()).resolves.toEqual(local)
    await expect(client.get('runtime:peer')).resolves.toEqual(local)
    await client.set(local, 'ssh:box')
    await client.patch({ activeRepoId: null, activeFileIdByWorktree: { a: null } })
    await client.flush()
    expect(invoke.mock.calls).toEqual([
      ['session_get', { hostId: null }],
      ['session_get', { hostId: 'runtime:peer' }],
      ['session_set', { args: local, hostId: 'ssh:box' }],
      ['session_patch', { args: { activeRepoId: null, activeFileIdByWorktree: { a: null } }, hostId: null }],
      ['session_flush', undefined]
    ])
  })

  it('orders writes and waits for their durable acknowledgements before flush', async () => {
    let release: (() => void) | undefined
    const held = new Promise<void>((resolve) => { release = resolve })
    const invoke = vi.fn(async (command: string) => {
      if (command === 'session_set') {
        await held
      }
    })
    const client = createTauriSessionClient(invoke)
    const first = client.set(getDefaultWorkspaceSession())
    const second = client.patch({ activeRepoId: 'repo-1' })
    const flushed = client.flush()
    await Promise.resolve()
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['session_set'])
    release!()
    await Promise.all([first, second, flushed])
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['session_set', 'session_patch', 'session_flush'])
  })

  it('flush exposes an earlier failed write and later writes can recover', async () => {
    const payload = {
      code: 'settings_write_failed', context: { operation: 'session_patch' },
      message: 'disk full', userMessage: 'The session could not be saved.'
    }
    const invoke = vi.fn(async () => undefined).mockRejectedValueOnce(payload)
    const client = createTauriSessionClient(invoke)
    await expect(client.patch({ activeRepoId: 'failed' })).rejects.toMatchObject({ code: payload.code })
    await expect(client.flush()).rejects.toMatchObject({ code: payload.code })
    expect(invoke).toHaveBeenCalledTimes(1)
    await client.patch({ activeRepoId: 'saved' })
    await expect(client.flush()).resolves.toBeUndefined()
    expect(invoke).toHaveBeenLastCalledWith('session_flush', undefined)
  })

  it('does not allow a read to overtake a queued mutation', async () => {
    const session = getDefaultWorkspaceSession()
    const invoke = vi.fn(async (command: string) => command === 'session_get' ? session : undefined)
    const client = createTauriSessionClient(invoke)
    const written = client.patch({ activeRepoId: null })
    await client.get()
    await written
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['session_patch', 'session_get'])
  })

  it('rejects malformed session responses instead of hydrating corrupt topology', async () => {
    for (const response of [undefined, {}, [], { ...getDefaultWorkspaceSession(), tabsByWorktree: [] }]) {
      const client = createTauriSessionClient(async () => response)
      await expect(client.get()).rejects.toMatchObject({ code: 'session_invalid_response' })
    }
  })

  it('reports unsupported synchronous operations without attempting an unsafe async save', () => {
    const invoke = vi.fn(async () => undefined)
    const client = createTauriSessionClient(invoke)
    expect(() => client.setSync(getDefaultWorkspaceSession())).toThrow('session_set_sync is unavailable')
    expect(client.readTerminalScrollback({ ref: 'snapshot' })).toBeNull()
    expect(invoke).not.toHaveBeenCalled()
  })

  it('prefetches sidecars before get resolves and refreshes their stable references', async () => {
    const reference = `v1-${'a'.repeat(32)}`
    const session = getDefaultWorkspaceSession()
    session.terminalLayoutsByTabId = {
      tab: { root: null, activeLeafId: null, expandedLeafId: null, scrollbackRefsByLeafId: { leaf: reference } }
    }
    let payload = 'saved terminal text'
    const invoke = vi.fn(async (command: string) => command === 'session_get' ? session : payload)
    const client = createTauriSessionClient(invoke)
    await client.get()
    expect(client.readTerminalScrollback({ ref: reference })).toBe(payload)
    expect(invoke).toHaveBeenLastCalledWith('session_read_terminal_scrollback', { reference })
    payload = 'new snapshot under the same reference'
    await client.get()
    expect(client.readTerminalScrollback({ ref: reference })).toBe(payload)
  })

  it('keeps independently loaded host snapshots and removes no-longer-referenced data', async () => {
    const localRef = `v1-${'a'.repeat(32)}`
    const remoteRef = `v1-${'b'.repeat(32)}`
    let localRefs: Record<string, string> = { leaf: localRef }
    const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
      if (command !== 'session_get') {
        return args?.reference
      }
      return {
        ...getDefaultWorkspaceSession(),
        terminalLayoutsByTabId: {
          tab: { root: null, activeLeafId: null, expandedLeafId: null,
            scrollbackRefsByLeafId: args?.hostId ? { leaf: remoteRef } : localRefs }
        }
      }
    })
    const client = createTauriSessionClient(invoke)
    await Promise.all([client.get(), client.get('runtime:peer')])
    expect(client.readTerminalScrollback({ ref: localRef })).toBe(localRef)
    expect(client.readTerminalScrollback({ ref: remoteRef })).toBe(remoteRef)
    localRefs = {}
    await client.get()
    expect(client.readTerminalScrollback({ ref: localRef })).toBeNull()
    expect(client.readTerminalScrollback({ ref: remoteRef })).toBe(remoteRef)
  })

  it('keeps last-good buffers during refetch and ignores an older same-host cache publication', async () => {
    const seedRef = `v1-${'a'.repeat(32)}`
    const oldRef = `v1-${'b'.repeat(32)}`
    const newRef = `v1-${'c'.repeat(32)}`
    let releaseOld: (() => void) | undefined
    const held = new Promise<void>((resolve) => { releaseOld = resolve })
    let sessionReads = 0
    const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
      if (command === 'session_get') {
        const reference = [seedRef, oldRef, newRef][sessionReads++]
        return {
          ...getDefaultWorkspaceSession(),
          terminalLayoutsByTabId: {
            tab: { root: null, activeLeafId: null, expandedLeafId: null,
              scrollbackRefsByLeafId: { leaf: reference } }
          }
        }
      }
      if (args?.reference === oldRef) {
        await held
      }
      return args?.reference
    })
    const client = createTauriSessionClient(invoke)
    await client.get()
    const oldSnapshot = client.get()
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('session_read_terminal_scrollback', { reference: oldRef })
    })
    expect(client.readTerminalScrollback({ ref: seedRef })).toBe(seedRef)
    await client.get()
    expect(client.readTerminalScrollback({ ref: newRef })).toBe(newRef)
    releaseOld!()
    await oldSnapshot
    expect(client.readTerminalScrollback({ ref: newRef })).toBe(newRef)
    expect(client.readTerminalScrollback({ ref: oldRef })).toBeNull()
  })
})
