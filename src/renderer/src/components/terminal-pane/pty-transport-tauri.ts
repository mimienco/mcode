// Tauri PTY transport — (a)-row slice of the seam (§2.1); capability optionals absent = not negotiated.
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { listen as tauriListen } from '@tauri-apps/api/event'
import { createTauriPtyClient, type PtyApi } from '../../desktop/pty-client'
import { extractIpcErrorMessage } from '@/lib/ipc-error'
import {
  drainRolledBackPtyShutdownData,
  ensurePtyDispatcher,
  ensurePtyDispatcherReady,
  isPtyDataHandlerShutdownPending,
  ptyDataHandlers,
  ptyExitHandlers,
  ptyReplayHandlers,
  ptyShutdownLifecycleHandlers,
  ptyTeardownHandlers,
  ptyWriteUnavailableHandlers
} from './pty-dispatcher'
import {
  drainPreHandlerPtyData,
  drainPreHandlerPtyExit,
  hasPreHandlerPtyExit
} from './pty-pre-handler-buffer'
import { createPtyInputWriteQueue } from './pty-input-write-queue'
import { createPtyOutputProcessor } from './pty-transport'
import { installDispatcherEventPort } from './pty-transport-tauri-dedupe'
import {
  unregisterPtyDataAndStatusHandlers,
  type OwnedDataAndReplay
} from './pty-transport-tauri-owned'
import type { PtyDataMeta } from './pty-dispatcher'
import type { IpcPtyTransportOptions, PtyTransport } from './pty-transport-types'

type TauriPtyInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

type TauriPtyListen = (
  event: string,
  listener: (payload: { payload: unknown }) => void
) => Promise<() => void>

export { applyPtySeqDedupe } from './pty-transport-tauri-dedupe'

/**
 * Builds the Tauri-side transport: installs the dispatcher event port and
 * returns the `PtyTransport` implementation over the typed pty client.
 */
export function createTauriPtyTransport(
  opts: IpcPtyTransportOptions = {},
  deps?: { invoke?: TauriPtyInvoke; listen?: TauriPtyListen; client?: PtyApi }
): PtyTransport {
  const {
    cwd,
    env,
    shellOverride,
    onPtyExit,
    onTitleChange,
    onPtySpawn,
    onBell,
    onAgentBecameIdle,
    onAgentBecameWorking,
    onAgentExited,
    onAgentStatus
  } = opts
  const client =
    deps?.client ?? createTauriPtyClient(deps?.invoke ?? tauriInvoke, deps?.listen ?? tauriListen)
  // Electron-only spawn fields (commandDelivery, launchConfig, resumeProviderSession, launchToken, launchAgent, startupCommandDelivery, initiallyHidden, admitPtyId, sessionId reattach) are ignored by design: (b)/(c) surface with no byte-owner mirror under the Tauri PTY layer yet (§2.1); connect always fresh-spawns and returns the plain handle string.
  installDispatcherEventPort(client)

  let connected = false
  let destroyed = false
  let ptyId: string | null = null
  let storedCallbacks: Parameters<PtyTransport['connect']>[0]['callbacks'] = {}
  const inputWriteQueue = createPtyInputWriteQueue({
    isWritable: (id) => connected && ptyId === id,
    write: (id, data) => client.write(Number(id), data),
    // Guard like the writeUnavailable handler: a rebind during the async drain must not tell the new pane that its write failed.
    onDrainFailure: (id) => {
      if (ptyId === id) {
        storedCallbacks.onWriteUnavailable?.()
      }
    }
  })
  // Why: no suppression wrapper — attention events are only muted during eager-buffer replay, which does not exist under Tauri until snapshots land.
  const outputProcessor = createPtyOutputProcessor({
    onTitleChange,
    onBell,
    onAgentBecameIdle,
    onAgentBecameWorking,
    onAgentExited,
    onAgentStatus
  })
  // Why: a new pane can attach to the same ptyId before the old instance's detach() runs; track owned handlers so unregister never deletes the live one.
  const ownedDataAndReplayHandlers = new Map<string, OwnedDataAndReplay>()
  const ownedExitHandlers = new Map<string, (code: number) => void>()

  function unregisterPtyHandlers(id: string): void {
    unregisterPtyDataAndStatusHandlers(id, ownedDataAndReplayHandlers)
    const ownedExit = ownedExitHandlers.get(id)
    if (ownedExit && ptyExitHandlers.get(id) === ownedExit) {
      ptyExitHandlers.delete(id)
    }
    ownedExitHandlers.delete(id)
    if (ptyTeardownHandlers.get(id) === clearAccumulatedState) {
      ptyTeardownHandlers.delete(id)
    }
    if (ptyShutdownLifecycleHandlers.get(id) === shutdownLifecycle) {
      ptyShutdownLifecycleHandlers.delete(id)
    }
  }

  function registerPtyDataHandler(id: string): void {
    // Why: route relay replay data through onReplayData so the replay guard stops xterm auto-replies from leaking into the shell.
    const replayHandler = (data: string): void => {
      if (ptyId !== id) {
        return
      }
      if (storedCallbacks.onReplayData) {
        storedCallbacks.onReplayData(data)
      } else {
        storedCallbacks.onData?.(data)
      }
    }
    ptyReplayHandlers.set(id, replayHandler)
    const dataHandler = (data: string, meta?: PtyDataMeta): void => {
      if (ptyId !== id) {
        return
      }
      outputProcessor.processData(data, storedCallbacks, {}, meta)
    }
    ptyDataHandlers.set(id, dataHandler)
    // Guard like the data/replay handlers: a rebind without detach must not leave a stale fan-out entry (would remount a healthy pane).
    const writeUnavailable = (): void => {
      if (ptyId === id) {
        storedCallbacks.onWriteUnavailable?.()
      }
    }
    ptyWriteUnavailableHandlers.set(id, writeUnavailable)
    ownedDataAndReplayHandlers.set(id, {
      data: dataHandler,
      replay: replayHandler,
      writeUnavailable
    })
    if (!isPtyDataHandlerShutdownPending(id)) {
      drainPreHandlerPtyData(id, dataHandler)
      drainRolledBackPtyShutdownData(id)
    }
  }

  function clearAccumulatedState(): void {
    outputProcessor.clearAccumulatedState()
  }
  const shutdownLifecycle = {
    pause: outputProcessor.pausePendingSideEffects,
    rollback: outputProcessor.flushPendingSideEffects,
    commit: clearAccumulatedState
  }
  function registerPtyExitHandler(id: string): boolean {
    const hadBufferedExit = hasPreHandlerPtyExit(id)
    const exitHandler = (code: number): void => {
      if (ptyId !== null && ptyId !== id) {
        // Why: a preserved session can report its old exit after this transport rebound to a replacement PTY.
        unregisterPtyHandlers(id)
        return
      }
      clearAccumulatedState()
      connected = false
      ptyId = null
      unregisterPtyHandlers(id)
      storedCallbacks.onExit?.(code)
      storedCallbacks.onDisconnect?.()
      onPtyExit?.(id)
    }
    ptyExitHandlers.set(id, exitHandler)
    ownedExitHandlers.set(id, exitHandler)
    // Why: shutdownWorktreeTerminals kills PTYs directly, bypassing disconnect/destroy; cancels stale-notification timers.
    ptyTeardownHandlers.set(id, clearAccumulatedState)
    ptyShutdownLifecycleHandlers.set(id, shutdownLifecycle)
    try {
      drainPreHandlerPtyExit(id, exitHandler)
    } catch (error) {
      if (!hadBufferedExit) {
        throw error
      }
      // Why: cleanup failure must not turn a delivered pre-attach exit into a connect rejection.
      console.error('[pty] buffered pre-attach exit cleanup failed', error)
    }
    return hadBufferedExit
  }

  return {
    async connect(options) {
      storedCallbacks = options.callbacks
      if (destroyed) {
        return
      }

      try {
        await ensurePtyDispatcherReady()
        if (destroyed) return
        const spawned = await client.spawn({
          cols: options.cols ?? 80,
          rows: options.rows ?? 24,
          ...(options.command ? { program: options.command } : {}),
          ...(cwd ? { cwd } : {}),
          ...(shellOverride ? { shellOverride } : {}),
          ...((options.env ?? env) ? { env: options.env ?? env } : {})
        })

        // Why: destroy mid-connect — kill the fresh spawn (no owner to retire it).
        if (destroyed) {
          await client.kill(spawned.id)
          return
        }

        ptyId = String(spawned.id)
        connected = true
        onPtySpawn?.(ptyId)

        registerPtyDataHandler(ptyId)
        registerPtyExitHandler(ptyId)
        if (!connected || ptyId !== String(spawned.id)) {
          return
        }
        storedCallbacks.onConnect?.()
        storedCallbacks.onStatus?.('shell')
        return ptyId
      } catch (err) {
        storedCallbacks.onError?.(extractIpcErrorMessage(err, String(err)))
        return undefined
      }
    },

    attach(options) {
      storedCallbacks = options.callbacks
      ensurePtyDispatcher()

      if (destroyed) {
        return
      }

      const id = options.existingPtyId
      ptyId = id
      connected = true
      // No eager-buffer replay under Tauri yet — no replay source until snapshots land (§2.1).
      registerPtyDataHandler(id)
      registerPtyExitHandler(id)
      if (!connected || ptyId !== id) {
        return
      }

      if (options.cols && options.rows) {
        client.resize(Number(id), options.cols, options.rows)
      }

      storedCallbacks.onConnect?.()
      storedCallbacks.onStatus?.('shell')
    },

    disconnect() {
      clearAccumulatedState()
      inputWriteQueue.clear()
      if (ptyId) {
        const id = ptyId
        // Bounded flush ladder runs Rust-side (pty_commands.rs).
        client.kill(Number(id))
        connected = false
        ptyId = null
        unregisterPtyHandlers(id)
        storedCallbacks.onDisconnect?.()
      }
    },

    detach(options) {
      // Why first: the successor transport owns the PTY after detach, and nothing below may throw its way past the census drop — a stranded gauge outlives the transport.
      outputProcessor.disposePendingSideEffectGauge()
      clearAccumulatedState()
      inputWriteQueue.clear()
      if (ptyId) {
        // Why: on remount keep the exit observer alive so a shell dying in the gap still clears stale tab/leaf bindings before reattach.
        if (options?.preserveExitObserver === false) {
          unregisterPtyHandlers(ptyId)
        } else {
          unregisterPtyDataAndStatusHandlers(ptyId, ownedDataAndReplayHandlers)
        }
      }
      connected = false
      ptyId = null
      storedCallbacks = {}
    },
    sendInput(data: string): boolean {
      if (!connected || !ptyId) {
        return false
      }
      return inputWriteQueue.enqueue(ptyId, data)
    },

    // Why: kept distinct from sendInput so the remote transport can override with flush-then-send (#7329); local queue drains same-turn.
    sendInputImmediate(data: string): boolean {
      if (!connected || !ptyId) {
        return false
      }
      return inputWriteQueue.enqueueQueryReply(ptyId, data)
    },

    resize(cols: number, rows: number): boolean {
      if (!connected || !ptyId) {
        return false
      }
      // No claimViewport under Tauri — viewport authority is (c) surface — so a meta.claim request degrades to the plain resize.
      client.resize(Number(ptyId), cols, rows)
      return true
    },

    isConnected() {
      return connected
    },

    getPtyId() {
      return ptyId
    },

    getConnectionId: () => null,

    getLocalSessionMetadata() {
      // Why: input routing/diagnostics must follow the launched PTY session, not later project setting changes.
      return {
        ...(cwd ? { cwd } : {}),
        ...(shellOverride ? { shellOverride } : {})
      }
    },

    destroy() {
      destroyed = true
      // Why finally: disconnect runs pty.kill IPC and consumer onDisconnect callbacks; a throw there must not strand the gauge in the very path where teardown already went wrong.
      try {
        this.disconnect()
      } finally {
        outputProcessor.disposePendingSideEffectGauge()
      }
    }
  }
}
