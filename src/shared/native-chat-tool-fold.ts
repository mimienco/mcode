import {
  isToolCallBlock,
  isToolResultBlock,
  type NativeChatBlock,
  type NativeChatMessage,
  type NativeChatToolCallBlock,
  type NativeChatToolResultBlock
} from './native-chat-types'

function isToolOnlyMessage(message: NativeChatMessage): boolean {
  return (
    message.blocks.length > 0 &&
    message.blocks.every((block) => isToolCallBlock(block) || isToolResultBlock(block))
  )
}

/** Fold consecutive tool-only messages into their preceding assistant turn. */
export function foldToolMessages(messages: readonly NativeChatMessage[]): NativeChatMessage[] {
  const output: NativeChatMessage[] = []
  let mutableAssistantIndex = -1
  for (const message of messages) {
    const previous = output.at(-1)
    if (isToolOnlyMessage(message) && previous?.role === 'assistant') {
      const index = output.length - 1
      if (mutableAssistantIndex !== index) {
        output[index] = { ...previous, blocks: [...previous.blocks] }
        mutableAssistantIndex = index
      }
      output[index]!.blocks.push(...message.blocks)
    } else {
      output.push(message)
      mutableAssistantIndex = -1
    }
  }
  return output
}

export type NativeChatToolFoldCache = {
  /** Fold with per-run caching: runs whose source messages are unchanged keep
   *  their folded identity, so downstream memoized rows skip re-rendering. */
  fold(messages: readonly NativeChatMessage[]): NativeChatMessage[]
}

/** Why: PERF-08 — the transcript re-folds on every stream tick; caching folded
 *  output per source run keeps earlier rows' identities stable across ticks so
 *  only the growing tail produces fresh objects. Every source reference is
 *  validated before a hit, so a replaced mid-run message can never serve stale
 *  blocks. */
export function createNativeChatToolFoldCache(): NativeChatToolFoldCache {
  const runs = new WeakMap<
    NativeChatMessage,
    { sources: readonly NativeChatMessage[]; folded: NativeChatMessage }
  >()
  return {
    fold(messages) {
      const output: NativeChatMessage[] = []
      let index = 0
      while (index < messages.length) {
        const previous = output.at(-1)
        const message = messages[index]!
        if (!(isToolOnlyMessage(message) && previous?.role === 'assistant')) {
          output.push(message)
          index += 1
          continue
        }
        let end = index
        while (end + 1 < messages.length && isToolOnlyMessage(messages[end + 1]!)) {
          end += 1
        }
        const sources = messages.slice(index, end + 1)
        const tailKey = sources.at(-1)!
        const cached = runs.get(tailKey)
        if (
          cached &&
          cached.sources.length === sources.length &&
          cached.sources.every((source, offset) => source === sources[offset])
        ) {
          // Same shape as foldToolMessages: the run replaces the trailing
          // assistant rather than appending after it.
          output.pop()
          output.push(cached.folded)
        } else {
          const folded = foldToolMessages([previous, ...sources])[0]!
          runs.set(tailKey, { sources, folded })
          output.pop()
          output.push(folded)
        }
        index = end + 1
      }
      return output
    }
  }
}

export type NativeChatToolPair = {
  call?: NativeChatToolCallBlock
  result?: NativeChatToolResultBlock
}

/** Pair calls and results by FIFO ordinal because transcript blocks carry no tool ids. */
export function pairToolBlocks(
  blocks: readonly NativeChatBlock[],
  limit = Infinity
): NativeChatToolPair[] {
  const pairs: NativeChatToolPair[] = []
  const callSlots: (number | null)[] = []
  let resultOrdinal = 0
  for (const block of blocks) {
    if (block.type === 'tool-call') {
      if (pairs.length < limit) {
        callSlots.push(pairs.length)
        pairs.push({ call: block })
      } else {
        callSlots.push(null)
      }
      continue
    }
    if (block.type !== 'tool-result') {
      continue
    }
    const slot = callSlots[resultOrdinal]
    if (slot === undefined) {
      if (pairs.length < limit) {
        pairs.push({ result: block })
      }
    } else {
      resultOrdinal += 1
      if (slot !== null) {
        pairs[slot]!.result = block
      }
    }
  }
  return pairs
}

export function splitNativeChatBlocks(blocks: readonly NativeChatBlock[]): {
  prose: NativeChatBlock[]
  tools: NativeChatBlock[]
} {
  const prose: NativeChatBlock[] = []
  const tools: NativeChatBlock[] = []
  for (const block of blocks) {
    if (isToolCallBlock(block) || isToolResultBlock(block)) {
      tools.push(block)
    } else {
      prose.push(block)
    }
  }
  return { prose, tools }
}
