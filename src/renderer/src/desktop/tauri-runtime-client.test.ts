import { describe, expect, it, vi } from 'vitest'
import { RuntimeRpcEnvelopeSchema } from '../../../shared/runtime-rpc-envelope'
import { RuntimeRpcCallError, unwrapRuntimeRpcResult } from '../runtime/runtime-rpc-result'
import { isRuntimeMethodNotFoundError } from '../store/slices/worktrees/listing/runtime-worktree-rpc-errors'
import { createRuntimeStatusHydration } from '../store/slices/runtime-status-hydration'
import { createTauriRuntimeClient, createTauriRuntimeEnvironmentsClient } from './tauri-runtime-client'

function environment() {
  return {
    id: 'persisted-owner', name: 'Server', createdAt: 1, updatedAt: 2,
    pairingRevision: 3, lastUsedAt: null, runtimeId: 'remote-runtime',
    endpoints: [{ id: 'ws-owner', kind: 'websocket', label: 'Remote', endpoint: 'wss://host/ws' }],
    preferredEndpointId: 'ws-owner'
  }
}

describe('Tauri runtime failure contracts', () => {
  it('returns valid local RPC failures accepted by the existing consumer and unsupported-method guard', async () => {
    const client = createTauriRuntimeClient()
    for (const method of ['status.get', 'worktree.detectedList', 'agent.cancelExec']) {
      const response = await client.call({ method, params: { secret: 'not-returned' } })
      expect(RuntimeRpcEnvelopeSchema.parse(response)).toEqual(response)
      expect(response).toMatchObject({ id: 'desktop-ipc', ok: false, error: { code: 'method_not_found' } })
      try {
        unwrapRuntimeRpcResult(response)
        throw new Error('must not succeed')
      } catch (error) {
        expect(error).toBeInstanceOf(RuntimeRpcCallError)
        expect(isRuntimeMethodNotFoundError(error)).toBe(true)
      }
      expect(JSON.stringify(response)).not.toContain('not-returned')
    }
  })

  it('never routes remote targets, including local-looking selectors, into native execution', async () => {
    const invoke = vi.fn()
    const client = createTauriRuntimeEnvironmentsClient(invoke)
    for (const selector of ['persisted-owner', 'local', 'localhost']) {
      const response = await client.call({ selector, method: 'status.get', params: { secret: 'not-returned' } })
      expect(RuntimeRpcEnvelopeSchema.parse(response)).toEqual(response)
      expect(() => unwrapRuntimeRpcResult(response)).toThrow(RuntimeRpcCallError)
      expect(response).toMatchObject({ ok: false, error: { code: 'runtime_unavailable' } })
      expect(await client.connect({ selector })).toEqual(response)
      expect(await client.getStatus({ selector })).toEqual(response)
    }
    expect(invoke).not.toHaveBeenCalled()
  })

  it('rejects direct status and mutation APIs without fabricated state or removal', async () => {
    await expect(createTauriRuntimeClient().getStatus()).rejects.toMatchObject({ code: 'method_not_found' })
    const client = createTauriRuntimeEnvironmentsClient(vi.fn())
    await expect(client.remove({ selector: 'persisted-owner' })).rejects.toMatchObject({ code: 'runtime_unavailable' })
    const onResponse = vi.fn()
    await expect(client.subscribe({ selector: 'persisted-owner', method: 'terminal.stream' }, { onResponse }))
      .rejects.toMatchObject({ code: 'runtime_unavailable' })
    expect(onResponse).not.toHaveBeenCalled()
  })

  it('reads the real public catalog and preserves host identities and pairing revisions', async () => {
    const invoke = vi.fn().mockResolvedValue([environment()])
    const listed = await createTauriRuntimeEnvironmentsClient(invoke).list()
    expect(listed).toEqual([environment()])
    expect(invoke).toHaveBeenCalledWith('runtime_environments_list', undefined)
  })

  it('rejects failed/malformed catalog reads instead of synthesizing an empty authoritative catalog', async () => {
    const client = createTauriRuntimeEnvironmentsClient(vi.fn().mockRejectedValue(new Error('unreadable')))
    await expect(client.list()).rejects.toThrow('unreadable')
    await expect(createTauriRuntimeEnvironmentsClient(vi.fn().mockResolvedValue(undefined)).list()).rejects.toThrow()
    await expect(createTauriRuntimeEnvironmentsClient(vi.fn().mockResolvedValue([{}])).list()).rejects.toThrow()
  })

  it('keeps previously restored remote owners when the real catalog read fails during hydration', async () => {
    const client = createTauriRuntimeEnvironmentsClient(vi.fn().mockRejectedValue(new Error('unreadable')))
    const previous = await createTauriRuntimeEnvironmentsClient(vi.fn().mockResolvedValue([environment()])).list()
    const publishEnvironments = vi.fn()
    const markCatalogSettled = vi.fn()
    const refreshEnvironmentStatus = vi.fn()
    const log = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    try {
      await createRuntimeStatusHydration({
        listEnvironments: client.list,
        getCurrentEnvironments: () => previous,
        publishEnvironments,
        refreshEnvironmentStatus,
        markCatalogSettled
      })()
      expect(publishEnvironments).not.toHaveBeenCalled()
      expect(refreshEnvironmentStatus).not.toHaveBeenCalled()
      expect(markCatalogSettled).toHaveBeenCalledOnce()
      expect(previous[0]?.id).toBe('persisted-owner')
    } finally {
      log.mockRestore()
    }
  })
})
