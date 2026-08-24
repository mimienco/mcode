import { describe, expect, it, vi } from 'vitest'
import type { AppState } from '../types'
import type { AgentStatusBatchUpdate } from './agent-status'
import { createTestStore } from './store-test-helpers'

// PERF-06: bursts of setAgentStatus updates used to clone the live status map
// (plus sibling maps) once per event even though a batch publishes once. The
// fold must reuse one private draft per map so work is O(events), not
// O(events x map-size).

const BASE = 4_000_000

function makeUpdate(paneKey: string, seq: number): AgentStatusBatchUpdate {
  return {
    kind: 'status',
    paneKey,
    payload: {
      state: seq % 2 === 0 ? 'working' : 'done',
      prompt: `prompt-${seq}`,
      agentType: 'pi'
    },
    timing: { updatedAt: BASE + seq }
  } as unknown as AgentStatusBatchUpdate
}

describe('agent status batch draft reuse', () => {
  it('reuses one live-map draft across a same-pane burst', () => {
    const store = createTestStore()
    const paneKey = 'tab-1:aaaaaaaa-1111-4111-8111-111111111111'
    const updates = Array.from({ length: 40 }, (_, seq) => makeUpdate(paneKey, seq))

    const distinctLiveRefs = new Set<object>()
    let publications = 0
    const unsubscribe = store.subscribe(() => {
      publications += 1
    })

    store.getState().transactAgentStatuses((transaction) => {
      for (const update of updates) {
        transaction.apply(update)
        distinctLiveRefs.add(transaction.getState().agentStatusByPaneKey)
      }
      return null
    })

    unsubscribe()

    // One privatized draft serves the whole burst instead of one copy per event.
    expect(distinctLiveRefs.size).toBeLessThanOrEqual(2)
    expect(publications).toBe(1)
    const finalEntry = store.getState().agentStatusByPaneKey[paneKey]
    expect(finalEntry.state).toBe('done')
    expect(finalEntry.prompt).toBe(`prompt-${updates.length - 1}`)
  })

  it('leaves untouched maps reference-stable while a sibling map is drafted', () => {
    const store = createTestStore()
    const activePane = 'tab-1:bbbbbbbb-2222-4222-8222-222222222222'
    const untouchedBefore = store.getState().retainedAgentsByPaneKey

    store.getState().transactAgentStatuses((transaction) => {
      for (let seq = 0; seq < 10; seq += 1) {
        transaction.apply(makeUpdate(activePane, seq))
      }
      return null
    })

    expect(store.getState().retainedAgentsByPaneKey).toBe(untouchedBefore)
  })

  it('keeps batched results identical to sequential application', () => {
    vi.useFakeTimers()
    vi.setSystemTime(BASE + 100)
    const sequentialStore = createTestStore()
    const batchStore = createTestStore()
    const panes = [
      'tab-1:cccccccc-3333-4333-8333-333333333333',
      'tab-1:dddddddd-4444-4444-8444-444444444444'
    ]
    const updates: AgentStatusBatchUpdate[] = []
    let seq = 0
    for (let round = 0; round < 6; round += 1) {
      for (const paneKey of panes) {
        updates.push(makeUpdate(paneKey, seq))
        seq += 1
      }
    }

    for (const update of updates) {
      if (update.kind === 'status') {
        sequentialStore
          .getState()
          .setAgentStatus(update.paneKey, update.payload, undefined, update.timing)
      }
    }
    batchStore.getState().setAgentStatuses(updates)

    const select = (state: AppState) => ({
      agentStatusByPaneKey: state.agentStatusByPaneKey,
      agentStatusEpoch: state.agentStatusEpoch,
      sortEpoch: state.sortEpoch
    })
    expect(select(batchStore.getState())).toEqual(select(sequentialStore.getState()))
  })
})
