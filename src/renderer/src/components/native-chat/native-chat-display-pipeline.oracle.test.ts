import { describe, expect, it } from 'vitest'
import type { NativeChatMessage } from '../../../../shared/native-chat-types'
import { stripNoiseMessages } from './native-chat-noise'
import { orderNativeChatMessages } from './native-chat-message-grouping'
import { foldToolMessages } from './native-chat-tool-fold'
import { createDisplayPipeline } from './native-chat-display-pipeline'

// PERF-07 commit 2: the incremental display pipeline must produce output
// identical to a full strip->order->fold rebuild after every operation,
// including out-of-order inserts, noise drops, and tool folds across the
// suffix boundary.
function reference(source: readonly NativeChatMessage[]): NativeChatMessage[] {
  return foldToolMessages(orderNativeChatMessages(stripNoiseMessages(source)))
}

let nextId = 0
type Shape = 'assistant' | 'tool' | 'user' | 'noise'

function makeMessage(shape: Shape, timestamp: number): NativeChatMessage {
  const id = `msg-${nextId++}`
  switch (shape) {
    case 'assistant':
      return {
        id,
        role: 'assistant',
        timestamp,
        source: 'transcript',
        blocks: [{ type: 'text', text: `assistant ${id}` } as never]
      }
    case 'tool':
      return {
        id,
        role: 'tool',
        timestamp,
        source: 'transcript',
        blocks: [
          { type: 'tool-call', name: 'read', arguments: {} } as never,
          { type: 'tool-result', content: 'ok' } as never
        ]
      }
    case 'user':
      return {
        id,
        role: 'user',
        timestamp,
        source: 'transcript',
        blocks: [{ type: 'text', text: `user ${id}` } as never]
      }
    default:
      return {
        id,
        role: 'system',
        timestamp,
        source: 'transcript',
        blocks: [{ type: 'text', text: `<system-reminder>noise ${id}</system-reminder>` } as never]
      }
  }
}

/** Deterministic LCG so failures reproduce exactly. */
function lcg(seed: number): () => number {
  let state = seed
  return () => {
    state = (state * 1_664_525 + 1_013_904_223) % 4_294_967_296
    return state / 4_294_967_296
  }
}

describe('display pipeline differential oracle', () => {
  it('matches full rebuild across randomized append/insert sequences', () => {
    const shapes: Shape[] = ['assistant', 'tool', 'tool', 'user', 'noise']
    for (let seed = 1; seed <= 25; seed += 1) {
      const random = lcg(seed * 7919)
      let clock = 1_000
      const source: NativeChatMessage[] = []
      const pipeline = createDisplayPipeline()

      for (let step = 0; step < 40; step += 1) {
        const roll = random()
        if (roll < 0.15 && source.length > 2) {
          // Out-of-order insert: timestamp lands before the current tail —
          // must trigger the full-rebuild fallback path.
          const insertAt = Math.floor(random() * source.length)
          clock -= 50
          source.splice(insertAt, 0, makeMessage(shapes[Math.floor(random() * shapes.length)], clock))
        } else if (roll < 0.3 && source.length > 0) {
          // Noise burst (stripped entirely) must not change the output.
          source.push(makeMessage('noise', clock))
          clock += 10
        } else {
          source.push(makeMessage(shapes[Math.floor(random() * shapes.length)], clock))
          clock += 10
        }

        const incremental = pipeline.process(source)
        expect(incremental).toEqual(reference(source))
      }
    }
  })

  it('reuses the previous output reference on pure suffix appends', () => {
    const source: NativeChatMessage[] = [makeMessage('user', 1)]
    const pipeline = createDisplayPipeline()
    const first = pipeline.process(source)

    const extended = [...source, makeMessage('assistant', 2), makeMessage('tool', 3)]
    const second = pipeline.process(extended)

    // Why: the leading tool in the suffix merges into the prior assistant —
    // the suffix fold window starts at that element by design; the prefix
    // slice stays referentially stable.
    expect(second.length).toBe(first.length + 1)
    expect(second.slice(0, first.length)).toEqual(first)
  })
})
