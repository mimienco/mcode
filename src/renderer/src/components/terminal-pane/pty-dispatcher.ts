/** Singleton PTY event dispatcher and eager buffer helpers, split out from pty-transport.ts. */
import {
  clearProcessedPtyCharTotal,
  deliverPtyDataWithDeferredAck,
  exposeE2eTerminalPtyAckGate,
  getProcessedPtyCharTotals
} from './terminal-pty-ack-gate'
import { bufferPreHandlerPtyData } from './pty-pre-handler-buffer'
import { eagerPtyHandles } from './pty-dispatcher-eager-buffer'
import { deliverPtyExitToHandlers } from './pty-exit-delivery'
import {
  clearReceivedPtyCharTotal,
  isPtyPushDeliveryBlackholed,
  recordPtyDataReceived,
  startTerminalDeliveryWatchdog
} from './terminal-delivery-watchdog'
import { recordTerminalFreezeBreadcrumb } from './terminal-freeze-breadcrumbs'
import { installTerminalFreezeReport } from './terminal-freeze-report'
import {
  bufferPtyShutdownData,
  bufferPtyShutdownReplayData,
  isPtyDataHandlerShutdownPending,
  ptyDataHandlers,
  ptyDataSidecars,
  ptyExitHandlers,
  ptyReplayHandlers
} from './pty-shutdown-data-suspension'
import { markCommittedPtyShutdowns } from './pty-shutdown-exit-deferral'
import { publishTerminalOutput } from '@/lib/terminal-output-observers'

export {
  ptyDataHandlers,
  ptyDataSidecars,
  ptyExitHandlers,
  ptyReplayHandlers,
  ptyShutdownLifecycleHandlers,
  ptyTeardownHandlers,
  drainRolledBackPtyShutdownData,
  isPtyDataHandlerShutdownPending,
  restorePtyDataHandlersAfterFailedShutdown,
  unregisterPtyDataHandlers
} from './pty-shutdown-data-suspension'

// ── Singleton PTY event dispatcher ───────────────────────────────────
// One global IPC listener per channel (routed by PTY ID) avoids the N-listener MaxListenersExceededWarning with many panes.

export type PtyDataMeta = {
  seq?: number
  rawLength?: number
  transformed?: boolean
  background?: boolean
  /** Main dropped this PTY's buffered output at the pending cap; repaint from the main-owned snapshot, not the live stream. */
  droppedOutput?: boolean
}

/** Sidecar PTY-data observers, invoked AFTER the primary handler so a side-effect-only watcher can't delay xterm rendering. */
/** Per-PTY replay handlers on a dedicated pty:replay channel so the renderer can engage the replay guard and suppress xterm auto-replies. */
const ptyExitSidecars = new Map<
  string,
  Set<(code: number, context: { hadPrimary: boolean }) => void>
>()
export const ptyWriteUnavailableHandlers = new Map<string, () => void>()

/**
 * Push-channel seam between the PTY transports and this router. `null` (the
 * default) subscribes through the Electron `window.api.pty` IPC channels; the
 * Tauri transport installs an adapter over its `PtyApi` events, fanning each
 * `PtyDataPayload` chunk out with the numeric Rust handle rendered as its
 * decimal string (`String(id)`) so every downstream map key stays a string.
 * Set once at Tauri transport construction; never swapped back except tests.
 */
export type PtyEventPort = {
  onData: (
    listener: (payload: {
      id: string
      data: string
      seq?: number
      rawLength?: number
      transformed?: boolean
    }) => void
  ) => Promise<() => void>
  onExit: (listener: (payload: { id: string; code: number }) => void) => Promise<() => void>
  onReplay: (listener: (payload: { id: string; data: string }) => void) => Promise<() => void>
}

let ptyDispatcherEventPort: PtyEventPort | null = null

export function setPtyDispatcherEventPort(port: PtyEventPort | null): void {
  ptyDispatcherEventPort = port
}
let ptyDispatcherAttached = false

let pushListenerUnsubscribes: (() => void)[] = []

/** Detach and re-subscribe every push-channel listener; called by the delivery watchdog on a confirmed wedge. */
export function reattachPtyDispatcherPushListeners(): void {
  recordTerminalFreezeBreadcrumb('push-listeners-reattach', {
    staleListenerCount: pushListenerUnsubscribes.length
  })
  const stale = pushListenerUnsubscribes
  pushListenerUnsubscribes = []
  for (const unsubscribe of stale) {
    unsubscribe()
  }
  // Why fire-and-forget: port subscriptions resolve asynchronously (the Electron
  // path resolves synchronously); the resolved unsubscribes are pushed onto the
  // live array inside attach, and a reattach before that resolve leaves the
  // stale subscription to resolve into the detached array it was pushed onto.
  void attachPtyPushListeners()
}

export function ensurePtyDispatcher(): void {
  if (ptyDispatcherAttached) {
    return
  }
  ptyDispatcherAttached = true
  exposeE2eTerminalPtyAckGate()
  installTerminalFreezeReport()
  attachPtyPushListeners()
  startTerminalDeliveryWatchdog({
    reattachPushListeners: reattachPtyDispatcherPushListeners,
    hasAttachedPtys: () => ptyDataHandlers.size > 0 || eagerPtyHandles.size > 0
  })
}

async function attachPtyPushListeners(): Promise<void> {
  const unsubscribes = pushListenerUnsubscribes
  if (ptyDispatcherEventPort) {
    unsubscribes.push(
      await ptyDispatcherEventPort.onData((payload) => {
        // Why: e2e-only wedge simulation — drop the chunk exactly like the field failure (no receive count, ACK, or dispatch).
        if (isPtyPushDeliveryBlackholed()) {
          return
        }
        handleDispatchedPtyData(payload)
      })
    )
  } else {
    unsubscribes.push(
      window.api.pty.onData((payload) => {
        // Why: e2e-only wedge simulation — drop the chunk exactly like the field failure (no receive count, ACK, or dispatch).
        if (isPtyPushDeliveryBlackholed()) {
          return
        }
        handleDispatchedPtyData(payload)
      })
    )
  }
  await attachPtySecondaryPushListeners(unsubscribes)
}

function handleDispatchedPtyData(payload: {
  id: string
  data: string
  seq?: number
  rawLength?: number
  transformed?: boolean
  background?: boolean
  droppedOutput?: boolean
}): void {
  let meta: PtyDataMeta | undefined
  if (typeof payload.seq === 'number') {
    meta ??= {}
    meta.seq = payload.seq
  }
  if (typeof payload.rawLength === 'number') {
    meta ??= {}
    meta.rawLength = payload.rawLength
  }
  if (payload.transformed === true) {
    meta ??= {}
    meta.transformed = true
  }
  if (payload.background === true) {
    meta ??= {}
    meta.background = true
  }
  if (payload.droppedOutput === true) {
    meta ??= {}
    meta.droppedOutput = true
  }
  const chars = payload.rawLength ?? payload.data.length
  const dispatch = (): void => {
    // Why first: the handler can synchronously publish OSC 133 completion.
    publishTerminalOutput({ ptyId: payload.id, data: payload.data })
    if (isPtyDataHandlerShutdownPending(payload.id)) {
      // Why: teardown output is speculative until the owner verifies sleep; retain it so a failed attempt resumes without losing terminal data.
      bufferPtyShutdownData(payload.id, payload.data, meta)
      return
    }
    const handler = ptyDataHandlers.get(payload.id)
    if (handler) {
      handler(payload.data, meta)
    } else {
      bufferPreHandlerPtyData(payload.id, payload.data, meta)
    }
    const sidecars = ptyDataSidecars.get(payload.id)
    if (sidecars && sidecars.size > 0) {
      // Why: snapshot before iterating — watchers often unsubscribe (or subscribe siblings) mid-iteration, and mutating the live Set would skip or double-fire.
      const snapshot = Array.from(sidecars)
      for (const watcher of snapshot) {
        watcher(payload.data)
      }
    }
  }
  recordPtyDataReceived(payload.id, chars)
  // Why deferred: main budgets by bytes PARSED not received; ACK fires when xterm consumes, and undelivered chunks settle at return so no PTY stays backpressured.
  deliverPtyDataWithDeferredAck(payload.id, chars, dispatch)
}

function handleDispatchedPtyExit(payload: {
  id: string
  code: number
  preserveRendererBinding?: boolean
}): void {
  if (payload.preserveRendererBinding === true) {
    // Why: host-initiated remote sleep has no requester transaction in this renderer; classify its ordered exit before pane cleanup runs.
    markCommittedPtyShutdowns([payload.id])
  }
  // Why: main drops its accounting on exit; drop totals too so a reused id restarts at zero on both sides.
  clearProcessedPtyCharTotal(payload.id)
  clearReceivedPtyCharTotal(payload.id)
  const sidecars = ptyExitSidecars.get(payload.id)
  if (sidecars) {
    ptyExitSidecars.delete(payload.id)
  }
  const primary = ptyExitHandlers.get(payload.id)
  if (primary) {
    // Why: one-shot owner — remove before invoking so a throwing callback can't stay registered for a duplicate exit.
    ptyExitHandlers.delete(payload.id)
  }
  deliverPtyExitToHandlers({
    ptyId: payload.id,
    code: payload.code,
    ...(primary ? { primary } : {}),
    sidecars: sidecars ? Array.from(sidecars) : []
  })
}

async function attachPtySecondaryPushListeners(unsubscribes: (() => void)[]): Promise<void> {
  if (ptyDispatcherEventPort) {
    unsubscribes.push(await ptyDispatcherEventPort.onExit(handleDispatchedPtyExit))
    unsubscribes.push(
      await ptyDispatcherEventPort.onReplay((payload) => {
        if (bufferPtyShutdownReplayData(payload.id, payload.data)) {
          return
        }
        ptyReplayHandlers.get(payload.id)?.(payload.data)
      })
    )
  } else {
    const unsubscribeWriteUnavailable = window.api.pty.onWriteUnavailable?.((payload) => {
      ptyWriteUnavailableHandlers.get(payload.id)?.()
    })
    if (unsubscribeWriteUnavailable) {
      unsubscribes.push(unsubscribeWriteUnavailable)
    }
    unsubscribes.push(
      window.api.pty.onReplay((payload) => {
        if (bufferPtyShutdownReplayData(payload.id, payload.data)) {
          return
        }
        ptyReplayHandlers.get(payload.id)?.(payload.data)
      })
    )
    unsubscribes.push(window.api.pty.onExit(handleDispatchedPtyExit))
  }
  // Deferred resync/ready channels are Electron-only: under the Tauri port
  // window.api.pty does not exist (and node-env tests may lack window itself),
  // so they are skipped instead of throwing.
  if (
    typeof window !== 'undefined' &&
    typeof window.api?.pty?.onDeliveryResyncRequest === 'function'
  ) {
    const unsubscribeResync = window.api.pty.onDeliveryResyncRequest((payload) => {
      window.api.pty?.respondDeliveryResync?.({
        requestId: payload.requestId,
        processedCharsByPty: getProcessedPtyCharTotals()
      })
    })
    if (unsubscribeResync) {
      unsubscribes.push(unsubscribeResync)
    }
  }
  // Why: tell main the pty:data listener is live; until it fires, bytes to a listener-less page are dropped-but-counted and pin the delivery gate.
  if (typeof window !== 'undefined') {
    window.api?.pty?.rendererDispatcherReady?.()
  }
}

export function subscribeToPtyExit(
  ptyId: string,
  watcher: (code: number, context: { hadPrimary: boolean }) => void
): () => void {
  ensurePtyDispatcher()
  let set = ptyExitSidecars.get(ptyId)
  if (!set) {
    set = new Set()
    ptyExitSidecars.set(ptyId, set)
  }
  set.add(watcher)
  return () => {
    const current = ptyExitSidecars.get(ptyId)
    if (!current) {
      return
    }
    current.delete(watcher)
    if (current.size === 0) {
      ptyExitSidecars.delete(ptyId)
    }
  }
}

// ─── Eager PTY buffer for reconnection on restart ────────────────────
// Why: PTYs spawn before TerminalPane mounts; buffer the early shell output (prompt/MOTD) so attach() can replay it.
// The eager pre-attach buffer lives in its own module now; re-exported here so
// existing `from './pty-dispatcher'` imports keep resolving.
export type { EagerPtyHandle } from './pty-dispatcher-eager-buffer'
export { getEagerPtyBufferHandle, registerEagerPtyBuffer } from './pty-dispatcher-eager-buffer'
