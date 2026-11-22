import assert from 'node:assert/strict'
import { expect, it as test, vi } from 'vitest'
import { DesktopCommandError } from './desktop-client'
import {
  AGENT_STATUS_CLEAR_EVENT,
  AGENT_STATUS_LEGACY_WORKER_TERMINAL_RECOVERY_EVENT,
  AGENT_STATUS_MIGRATION_UNSUPPORTED_CLEAR_EVENT,
  AGENT_STATUS_MIGRATION_UNSUPPORTED_EVENT,
  AGENT_STATUS_SET_EVENT,
  createTauriAgentStatusClient
} from './tauri-agent-status-client'
import type { AgentStatusIpcPayload } from '../../../shared/agent-status-types'
import type { AgentInterruptInferenceRequest } from '../../../shared/agent-interrupt-intent'
import type { AgentQuestionAnsweredInferenceRequest } from '../../../shared/agent-question-answered-intent'

type RecordedListener = (event: { payload: unknown }) => void

function recordedTransport(handlers: Record<string, (args?: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler(args)
  }
  const listeners = new Map<string, RecordedListener[]>()
  const listen = async (event: string, listener: RecordedListener) => {
    const bucket = listeners.get(event) ?? []
    bucket.push(listener)
    listeners.set(event, bucket)
    return () => {
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((entry) => entry !== listener)
      )
    }
  }
  const emit = (event: string, payload: unknown): void => {
    for (const listener of listeners.get(event) ?? []) {
      listener({ payload })
    }
  }
  return { calls, invoke, listen, emit }
}

const flush = (): Promise<void> => Promise.resolve()

function statusRow(overrides: Partial<AgentStatusIpcPayload> = {}): AgentStatusIpcPayload {
  return {
    state: 'working',
    prompt: 'do the thing',
    paneKey: 'tab-1:leaf',
    connectionId: null,
    receivedAt: 1_000,
    stateStartedAt: 900,
    ...overrides
  }
}

const interruptRequest: AgentInterruptInferenceRequest = {
  paneKey: 'tab-1:leaf',
  baselineUpdatedAt: 1_000,
  baselineStateStartedAt: 900,
  baselinePrompt: 'do the thing',
  baselineAgentType: 'claude',
  intent: 'plain-escape'
}

const questionRequest: AgentQuestionAnsweredInferenceRequest = {
  paneKey: 'tab-1:leaf',
  baselineUpdatedAt: 1_000,
  baselineStateStartedAt: 900,
  baselinePrompt: 'do the thing',
  baselineAgentType: 'claude'
}

test('event names match the preload agentStatus channels byte-for-byte', () => {
  assert.equal(AGENT_STATUS_SET_EVENT, 'agentStatus:set')
  assert.equal(AGENT_STATUS_CLEAR_EVENT, 'agentStatus:clear')
  assert.equal(AGENT_STATUS_MIGRATION_UNSUPPORTED_EVENT, 'agentStatus:migrationUnsupported')
  assert.equal(
    AGENT_STATUS_MIGRATION_UNSUPPORTED_CLEAR_EVENT,
    'agentStatus:migrationUnsupportedClear'
  )
  assert.equal(
    AGENT_STATUS_LEGACY_WORKER_TERMINAL_RECOVERY_EVENT,
    'agentStatus:legacyWorkerTerminalRecovery'
  )
})

test('onSet delivers valid payloads and drops malformed ones', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriAgentStatusClient(invoke, listen)
  const received = vi.fn()
  client.onSet(received)
  await flush()

  const row = statusRow({ agentType: 'claude' })
  emit(AGENT_STATUS_SET_EVENT, row)
  emit(AGENT_STATUS_SET_EVENT, { paneKey: 'tab-1:leaf' })
  emit(AGENT_STATUS_SET_EVENT, null)
  emit(AGENT_STATUS_SET_EVENT, {
    ...row,
    state: 'running'
  })
  emit(AGENT_STATUS_SET_EVENT, { ...row, connectionId: 12 })

  expect(received.mock.calls).toEqual([[row]])
})

test('onClear accepts paneKey and transient shapes and drops the rest', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriAgentStatusClient(invoke, listen)
  const received = vi.fn()
  client.onClear(received)
  await flush()

  emit(AGENT_STATUS_CLEAR_EVENT, { paneKey: 'tab-1:leaf' })
  emit(AGENT_STATUS_CLEAR_EVENT, {
    transient: true,
    connectionId: 'ssh-1',
    clearedAt: 42
  })
  emit(AGENT_STATUS_CLEAR_EVENT, { transient: true, connectionId: '', clearedAt: 1 })
  emit(AGENT_STATUS_CLEAR_EVENT, { paneKey: '' })
  emit(AGENT_STATUS_CLEAR_EVENT, { bogus: true })

  expect(received.mock.calls).toEqual([
    [{ paneKey: 'tab-1:leaf' }],
    [{ transient: true, connectionId: 'ssh-1', clearedAt: 42 }]
  ])
})

test('migration and legacy-recovery listeners guard their payloads', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriAgentStatusClient(invoke, listen)
  const unsupported = vi.fn()
  const unsupportedClear = vi.fn()
  const recovery = vi.fn()
  client.onMigrationUnsupported(unsupported)
  client.onMigrationUnsupportedClear(unsupportedClear)
  client.onLegacyWorkerTerminalRecovery(recovery)
  await flush()

  const entry = {
    ptyId: 'pty-1',
    reason: 'legacy-numeric-pane-key' as const,
    source: 'local' as const,
    updatedAt: 9
  }
  emit(AGENT_STATUS_MIGRATION_UNSUPPORTED_EVENT, entry)
  emit(AGENT_STATUS_MIGRATION_UNSUPPORTED_EVENT, { ...entry, reason: 'other' })
  emit(AGENT_STATUS_MIGRATION_UNSUPPORTED_CLEAR_EVENT, { ptyId: 'pty-1' })
  emit(AGENT_STATUS_MIGRATION_UNSUPPORTED_CLEAR_EVENT, { ptyId: '' })
  emit(AGENT_STATUS_LEGACY_WORKER_TERMINAL_RECOVERY_EVENT, {
    paneKey: 'tab-1:leaf',
    resolution: 'adopted',
    ptyId: 'pty-1'
  })
  emit(AGENT_STATUS_LEGACY_WORKER_TERMINAL_RECOVERY_EVENT, {
    paneKey: 'tab-1:leaf',
    resolution: 'kept'
  })

  expect(unsupported.mock.calls).toEqual([[entry]])
  expect(unsupportedClear.mock.calls).toEqual([[{ ptyId: 'pty-1' }]])
  expect(recovery.mock.calls).toEqual([
    [{ paneKey: 'tab-1:leaf', resolution: 'adopted', ptyId: 'pty-1' }]
  ])
})

test('unsubscribe is synchronous and stops delivery', async () => {
  const { invoke, listen, emit } = recordedTransport({})
  const client = createTauriAgentStatusClient(invoke, listen)
  const received = vi.fn()
  const off = client.onSet(received)
  await flush()

  emit(AGENT_STATUS_SET_EVENT, statusRow())
  off()
  emit(AGENT_STATUS_SET_EVENT, statusRow({ prompt: 'later' }))
  expect(received).toHaveBeenCalledTimes(1)
})

test('unsubscribe before listen arming drops events and detaches', async () => {
  let resolveListen: ((off: () => void) => void) | undefined
  const unlisten = vi.fn()
  const listeners: RecordedListener[] = []
  const listen = (_event: string, listener: RecordedListener): Promise<() => void> => {
    listeners.push(listener)
    return new Promise((resolve) => {
      resolveListen = () => resolve(unlisten)
    })
  }
  const client = createTauriAgentStatusClient(async () => undefined, listen)
  const received = vi.fn()
  const off = client.onSet(received)
  expect(typeof off).toBe('function')
  off()
  listeners[0]?.({ payload: statusRow() })
  expect(received).not.toHaveBeenCalled()

  resolveListen?.(unlisten)
  await flush()
  expect(unlisten).toHaveBeenCalledTimes(1)
})

test('getSnapshot maps onto agent_status_get_snapshot and drops malformed rows', async () => {
  const good = statusRow({ prompt: 'live' })
  const { calls, invoke } = recordedTransport({
    agent_status_get_snapshot: () => [good, { paneKey: 'nope' }, null]
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)

  await expect(client.getSnapshot()).resolves.toEqual([good])
  expect(calls).toEqual([{ command: 'agent_status_get_snapshot', args: undefined }])
})

test('getSnapshot rejects a non-array payload as DesktopCommandError', async () => {
  const { invoke } = recordedTransport({
    agent_status_get_snapshot: () => ({ entries: [] })
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)
  const error = await client.getSnapshot().then(
    () => null,
    (thrown: unknown) => thrown
  )
  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'agent_status_get_snapshot_invalid_response')
})

test('infer commands pass camelCase request objects and require a boolean', async () => {
  const { calls, invoke } = recordedTransport({
    agent_status_infer_interrupt: () => true,
    agent_status_infer_question_answered: () => false
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)

  await expect(client.inferInterrupt(interruptRequest)).resolves.toBe(true)
  await expect(client.inferQuestionAnswered(questionRequest)).resolves.toBe(false)
  expect(calls).toEqual([
    { command: 'agent_status_infer_interrupt', args: { request: interruptRequest } },
    { command: 'agent_status_infer_question_answered', args: { request: questionRequest } }
  ])
})

test('inferInterrupt rejects a non-boolean as DesktopCommandError', async () => {
  const { invoke } = recordedTransport({
    agent_status_infer_interrupt: () => 'yes'
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)
  await expect(client.inferInterrupt(interruptRequest)).rejects.toMatchObject({
    code: 'agent_status_infer_interrupt_invalid_response'
  })
})

test('invoke failures become DesktopCommandError', async () => {
  const { invoke } = recordedTransport({
    agent_status_get_snapshot: () => {
      throw {
        code: 'agent_status_store_poisoned',
        context: { operation: 'agent_status_get_snapshot' },
        message: 'status store lock poisoned',
        userMessage: ''
      }
    }
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)
  const error = await client.getSnapshot().then(
    () => null,
    (thrown: unknown) => thrown
  )
  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'agent_status_store_poisoned')
})

test('getMigrationUnsupportedSnapshot filters malformed entries', async () => {
  const entry = {
    ptyId: 'pty-1',
    reason: 'legacy-numeric-pane-key' as const,
    source: 'ssh' as const,
    updatedAt: 3
  }
  const { calls, invoke } = recordedTransport({
    agent_status_get_migration_unsupported_snapshot: () => [entry, { ptyId: 'x' }]
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)
  await expect(client.getMigrationUnsupportedSnapshot()).resolves.toEqual([entry])
  expect(calls).toEqual([
    { command: 'agent_status_get_migration_unsupported_snapshot', args: undefined }
  ])
})

test('drop family is fire-and-forget onto the camelCase commands', async () => {
  const { calls, invoke } = recordedTransport({
    agent_status_drop: () => null,
    agent_status_drop_by_tab_prefix: () => null,
    agent_status_retire_pane_authority: () => null,
    agent_status_restore_pane_authority: () => true,
    agent_status_transfer_pane_authority: () => true
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)

  assert.equal(client.drop('tab-1:leaf'), undefined)
  assert.equal(client.dropByTabPrefix('tab-1'), undefined)
  assert.equal(client.retirePaneAuthority('tab-1:leaf'), undefined)
  assert.equal(client.restorePaneAuthority('tab-1:leaf'), undefined)
  assert.equal(
    client.transferPaneAuthority({
      fromPaneKey: 'tab-1:leaf',
      toPaneKey: 'tab-2:leaf',
      ptyId: 'pty-1'
    }),
    undefined
  )
  await flush()

  expect(calls).toEqual([
    { command: 'agent_status_drop', args: { paneKey: 'tab-1:leaf' } },
    { command: 'agent_status_drop_by_tab_prefix', args: { tabId: 'tab-1' } },
    { command: 'agent_status_retire_pane_authority', args: { paneKey: 'tab-1:leaf' } },
    { command: 'agent_status_restore_pane_authority', args: { paneKey: 'tab-1:leaf' } },
    {
      command: 'agent_status_transfer_pane_authority',
      args: { fromPaneKey: 'tab-1:leaf', toPaneKey: 'tab-2:leaf', ptyId: 'pty-1' }
    }
  ])
})

test('fire-and-forget drop does not surface invoke failures', async () => {
  const { invoke } = recordedTransport({
    agent_status_drop: () => {
      throw new Error('store poisoned')
    }
  })
  const client = createTauriAgentStatusClient(invoke, async () => () => undefined)
  expect(() => client.drop('tab-1:leaf')).not.toThrow()
  await flush()
})
