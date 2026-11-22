import type { NativeChatMessage } from '../../../../shared/native-chat-types'
import { stripNoiseMessages } from './native-chat-noise'
import { orderNativeChatMessages } from './native-chat-message-grouping'
import { isToolCallBlock, isToolResultBlock } from '../../../../shared/native-chat-types'

// Why: PERF-07 — strip/order/fold over the whole transcript ran on every
// stream frame because appends change array identity. This pipeline re-folds
// only the suffix when the input extends the previous one without ordering
// regressions, and falls back to a full rebuild otherwise. The oracle
// differential test locks incremental output === full-rebuild output.

type DisplayCache = {
  source: readonly NativeChatMessage[]
  output: NativeChatMessage[]
  /** Source index where the last output element begins (fold-merge boundary). */
  lastOutputSourceStart: number
}

/** Index-aware twin of shared foldToolMessages: identical merge rule, plus the
 *  source index each output element starts at, so suffix windows can begin at
 *  fold-merge boundaries. Kept in lockstep by the oracle test. */
function foldWithSourceStarts(
  source: readonly NativeChatMessage[],
  startIndex: number
): {
  output: NativeChatMessage[]
  starts: number[]
} {
  const output: NativeChatMessage[] = []
  const starts: number[] = []
  let mutableAssistantIndex = -1
  for (let index = 0; index < source.length; index += 1) {
    const message = source[index]
    const previous = output.at(-1)
    if (isToolOnly(message) && previous?.role === 'assistant') {
      const target = output.length - 1
      if (mutableAssistantIndex !== target) {
        output[target] = { ...previous, blocks: [...previous.blocks] }
        mutableAssistantIndex = target
      }
      output[target]!.blocks.push(...message.blocks)
      continue
    }
    output.push(message)
    starts.push(startIndex + index)
    mutableAssistantIndex = -1
  }
  return { output, starts }
}

function isToolOnly(message: NativeChatMessage): boolean {
  return (
    message.blocks.length > 0 &&
    message.blocks.every((block) => isToolCallBlock(block) || isToolResultBlock(block))
  )
}

function buildFull(source: readonly NativeChatMessage[]): DisplayCache {
  const cleaned = stripNoiseMessages(source)
  const ordered = orderNativeChatMessages(cleaned)
  // Why: order is a pure comparator sort — recover each surviving output
  // element's original source index so suffix windows start at fold boundaries.
  const sourceIndexByOriginal = new Map<NativeChatMessage, number>()
  source.forEach((message, index) => sourceIndexByOriginal.set(message, index))
  const orderedStarts = ordered.map(
    (message) => sourceIndexByOriginal.get(message) ?? 0
  )
  // Re-run the same linear fold over the ordered list while carrying starts.
  const output: NativeChatMessage[] = []
  const starts: number[] = []
  let mutableAssistantIndex = -1
  for (let index = 0; index < ordered.length; index += 1) {
    const message = ordered[index]
    const previous = output.at(-1)
    if (isToolOnly(message) && previous?.role === 'assistant') {
      const target = output.length - 1
      if (mutableAssistantIndex !== target) {
        output[target] = { ...previous, blocks: [...previous.blocks] }
        mutableAssistantIndex = target
      }
      output[target]!.blocks.push(...message.blocks)
      continue
    }
    output.push(message)
    starts.push(orderedStarts[index])
    mutableAssistantIndex = -1
  }
  return {
    source,
    output,
    lastOutputSourceStart: starts.at(-1) ?? 0
  }
}

function sharesPrefix(
  next: readonly NativeChatMessage[],
  previous: readonly NativeChatMessage[],
  length: number
): boolean {
  for (let index = 0; index < length; index += 1) {
    if (next[index] !== previous[index]) {
      return false
    }
  }
  return true
}

export function createDisplayPipeline(): {
  process: (source: readonly NativeChatMessage[]) => NativeChatMessage[]
} {
  let cache: DisplayCache | null = null

  function commit(source: readonly NativeChatMessage[], output: NativeChatMessage[], lastOutputSourceStart: number): NativeChatMessage[] {
    cache = { source, output, lastOutputSourceStart }
    return output
  }

  return {
    process: (source) => {
      if (cache?.source === source) {
        return cache.output
      }
      const previous = cache
      const canExtend =
        previous !== null &&
        source.length > previous.source.length &&
        sharesPrefix(source, previous.source, previous.source.length)

      if (!canExtend || !previous) {
        const built = buildFull(source)
        return commit(source, built.output, built.lastOutputSourceStart)
      }

      // Ordering-regression guard: any new message sorting before the previous
      // tail can land anywhere in the output — fall back to a full rebuild.
      const previousTail = previous.source[previous.source.length - 1]
      const newMessages = source.slice(previous.source.length)
      const orderSafe = newMessages.every((message) => {
        if (message.timestamp === null || previousTail.timestamp === null) {
          return false
        }
        return message.timestamp >= previousTail.timestamp
      })
      if (!orderSafe) {
        const built = buildFull(source)
        return commit(source, built.output, built.lastOutputSourceStart)
      }

      // Why: PERF-07 — re-fold from the last output element's source position:
      // a leading tool-only message in the suffix merges into exactly that
      // element, matching what a full rebuild produces.
      const windowStart = previous.lastOutputSourceStart
      const window = source.slice(windowStart)
      const cleaned = stripNoiseMessages(window)
      const ordered = orderNativeChatMessages(cleaned)
      const folded = foldWithSourceStarts(ordered, windowStart)
      const output = [...previous.output.slice(0, previous.output.length - 1), ...folded.output]
      const lastStart = folded.starts.at(-1) ?? previous.lastOutputSourceStart
      return commit(source, output, lastStart)
    }
  }
}
