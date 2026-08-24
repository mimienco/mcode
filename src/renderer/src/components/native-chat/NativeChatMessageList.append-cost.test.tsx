// @vitest-environment happy-dom

import '@testing-library/jest-dom/vitest'

import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import type { NativeChatMessage } from '../../../../shared/native-chat-types'
import { NativeChatMessageList } from './NativeChatMessageList'

// PERF-07 commit-3 gate: with MessageRow memoized and prose cached, appending
// to a 5000-message history must stay far below the 50 ms long-task budget —
// this decides whether the incremental fold pipeline is required at all.
afterEach(cleanup)

const TOTAL = 5000

function makeMessages(count: number): NativeChatMessage[] {
  const messages: NativeChatMessage[] = []
  for (let index = 0; index < count; index += 1) {
    messages.push({
      id: `m-${index}`,
      role: index % 2 === 0 ? 'user' : 'assistant',
      timestamp: index + 1,
      source: 'transcript',
      blocks: [{ type: 'text', text: `${index % 2 === 0 ? 'prompt' : 'reply'} ${index}` }]
    })
  }
  return messages
}

function makeSession(
  messages: NativeChatMessage[]
): Parameters<typeof NativeChatMessageList>[0]['session'] {
  return {
    agent: 'codex',
    sessionId: 'session-perf',
    status: 'ready',
    messages,
    hasMore: false,
    loadingEarlier: false,
    loadEarlier: () => undefined,
    readPhase: 'ready'
  }
}

// Why: gated — this is the PERF-07 commit-3 (virtualization) RED gate. It
// fails until the list renders a bounded window instead of the full history.
// Run with MCODE_PERF07_GATE=1 to check the SLO after implementing it.
describe.skipIf(!process.env.MCODE_PERF07_GATE)('native chat append cost at 5000 messages', () => {
  it('keeps tail-append rerenders well under the 50 ms long-task budget', () => {
    let messages = makeMessages(TOTAL)
    const view = render(
      <NativeChatMessageList
        session={makeSession(messages)}
        isWorking={false}
        expandSignal={false}
        fontScale={1}
      />
    )
    expect(screen.getAllByText(/reply 4999/).length).toBeGreaterThan(0)

    const durations: number[] = []
    for (let append = 0; append < 6; append += 1) {
      const id = `m-${TOTAL + append}`
      messages = [
        ...messages,
        {
          id,
          role: 'assistant',
          timestamp: TOTAL + append + 1,
          source: 'transcript',
          blocks: [{ type: 'text', text: `stream chunk ${append}` }]
        }
      ]
      const startedAt = performance.now()
      view.rerender(
        <NativeChatMessageList
          session={makeSession(messages)}
          isWorking={false}
          expandSignal={false}
          fontScale={1}
        />
      )
      durations.push(performance.now() - startedAt)
      if (!view.container.textContent?.includes(`stream chunk ${append}`)) {
        throw new Error(`append ${append} missing after rerender`)
      }
    }

    const worst = Math.max(...durations)
    // Why: generous CI slack, but still an order of magnitude below the 50 ms
    // long-task budget — a full-history re-render blows straight past this.
    expect(worst).toBeLessThan(50)
  })
})
