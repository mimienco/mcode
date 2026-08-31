/** Eager pre-attach PTY buffering, split from pty-dispatcher.ts for the max-lines gate. */
import { TERMINAL_SCROLLBACK_SESSION_BUFFER_BYTE_LIMIT } from '../../../../shared/terminal-scrollback-limits'
import { clampUtf8Tail, type EagerBufferChunk } from './pty-eager-buffer-clamp'
import {
  clearPreHandlerPtyState,
  drainPreHandlerPtyData,
  drainPreHandlerPtyExit
} from './pty-pre-handler-buffer'
import { ptyDataHandlers, ptyExitHandlers, ptyReplayHandlers } from './pty-shutdown-data-suspension'
// Why: cycle with pty-dispatcher is intentional — `ensurePtyDispatcher` is a
// hoisted function declaration and only invoked at runtime, never at module init.
import { ensurePtyDispatcher } from './pty-dispatcher'

export type EagerPtyHandle = { flush: () => string; dispose: () => void }

export const eagerPtyHandles = new Map<string, EagerPtyHandle>()

export function getEagerPtyBufferHandle(ptyId: string): EagerPtyHandle | undefined {
  return eagerPtyHandles.get(ptyId)
}

const EAGER_BUFFER_MAX_BYTES = TERMINAL_SCROLLBACK_SESSION_BUFFER_BYTE_LIMIT

export function registerEagerPtyBuffer(
  ptyId: string,
  onExit: (ptyId: string, code: number) => void
): EagerPtyHandle {
  ensurePtyDispatcher()
  // Why: head index instead of Array.shift() (O(n)) so pre-attach buffering isn't quadratic under many small chunks.
  const chunks: EagerBufferChunk[] = []
  let head = 0
  let bufferBytes = 0

  const dataHandler = (data: string): void => {
    // Why: a single over-cap chunk would bypass the trim loop below; keep only its most-recent tail.
    const chunk = clampUtf8Tail(data, EAGER_BUFFER_MAX_BYTES)
    chunks.push(chunk)
    bufferBytes += chunk.bytes
    // Drop whole leading chunks (keeping the prompt-bearing tail) until within cap.
    while (bufferBytes > EAGER_BUFFER_MAX_BYTES && head < chunks.length - 1) {
      bufferBytes -= chunks[head].bytes
      chunks[head] = { data: '', bytes: 0 }
      head += 1
    }
    // Compact when dead slots reach half the array so it can't grow unbounded.
    if (head > 0 && head * 2 >= chunks.length) {
      chunks.splice(0, head)
      head = 0
    }
  }
  const exitHandler = (code: number): void => {
    // Shell died before attach; identity-guard so we never evict a handler a transport re-registered for this id (#7894 detach/attach race).
    if (ptyDataHandlers.get(ptyId) === dataHandler) {
      ptyDataHandlers.delete(ptyId)
      ptyReplayHandlers.delete(ptyId)
    }
    ptyExitHandlers.delete(ptyId)
    eagerPtyHandles.delete(ptyId)
    onExit(ptyId, code)
  }

  ptyDataHandlers.set(ptyId, dataHandler)
  ptyExitHandlers.set(ptyId, exitHandler)

  const handle: EagerPtyHandle = {
    flush() {
      const data = chunks
        .slice(head)
        .map((chunk) => chunk.data)
        .join('')
      chunks.length = 0
      head = 0
      bufferBytes = 0
      return data
    },
    dispose() {
      // Why: identity-guard removal — after attach() swaps in its own handler this must no-op, not evict it.
      if (ptyDataHandlers.get(ptyId) === dataHandler) {
        ptyDataHandlers.delete(ptyId)
        ptyReplayHandlers.delete(ptyId)
      }
      if (ptyExitHandlers.get(ptyId) === exitHandler) {
        ptyExitHandlers.delete(ptyId)
      }
      eagerPtyHandles.delete(ptyId)
    }
  }

  eagerPtyHandles.set(ptyId, handle)
  drainPreHandlerPtyData(ptyId, dataHandler)
  // Why: defer the pre-handler exit one microtask so the caller receives the returned handle before onExit fires.
  queueMicrotask(() => {
    if (ptyExitHandlers.get(ptyId) === exitHandler) {
      drainPreHandlerPtyExit(ptyId, exitHandler)
    } else {
      clearPreHandlerPtyState(ptyId)
    }
  })
  return handle
}
