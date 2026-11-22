import type { PtyApi } from '../../../preload/api/pty-api'
import { DesktopCommandError, type DesktopInvoke } from './desktop-client'
import { createTauriPtyClient } from './pty-client'

type EventSource = Parameters<typeof createTauriPtyClient>[1]

function unsupported(operation: string): DesktopCommandError {
  return new DesktopCommandError('pty_operation_unsupported', { operation },
    `${operation} is not available in this Tauri build.`, 'This terminal operation is not available yet.')
}

function nativeId(id: string): number {
  const value = Number(id)
  if (!/^[1-9][0-9]*$/.test(id) || !Number.isSafeInteger(value)) {
    throw new DesktopCommandError('pty_invalid_id', { operation: 'pty_id' },
      'The PTY is not owned by the local Tauri runtime.', 'This terminal belongs to another runtime.')
  }
  return value
}

// Shared renderer code still uses preload calls alongside the runtime-selected native transport.
export function createTauriPtyWindowClient(invoke: DesktopInvoke, listen: EventSource) {
  const native = createTauriPtyClient(invoke, listen)
  const warned = new Set<string>()
  function warn(operation: string): void {
    if (!warned.has(operation)) {
      warned.add(operation)
      console.warn(`[tauri-pty] ${operation} is unsupported; no terminal authority was changed.`)
    }
  }
  function fire(operation: string, action: () => Promise<unknown>): void {
    void Promise.resolve().then(action).catch((error: unknown) => {
      console.warn(`[tauri-pty] ${operation} failed`, error)
    })
  }
  const reject = async (operation: string): Promise<never> => { throw unsupported(operation) }
  const noAuthority = (operation: string) => (): void => warn(operation)
  const noEvents = (operation: string) => (): (() => void) => { warn(operation); return () => undefined }
  function subscribe<T>(register: (listener: (value: T) => void) => Promise<() => void>, listener: (value: T) => void): () => void {
    let disposed = false
    let detach: (() => void) | undefined
    void register((value) => { if (!disposed) { listener(value) } }).then((unlisten) => {
      if (disposed) { unlisten() } else { detach = unlisten }
    }).catch((error: unknown) => console.warn('[tauri-pty] listener registration failed', error))
    return () => { disposed = true; detach?.(); detach = undefined }
  }

  return {
    write: (id, data) => fire('pty_write', () => native.write(nativeId(id), data)),
    writeAccepted: async (id, data) => { await native.write(nativeId(id), data); return true },
    resize: (id, cols, rows) => fire('pty_resize', () => native.resize(nativeId(id), cols, rows)),
    hasPty: async (id) => native.hasPty(nativeId(id)),
    getSize: async (id) => native.getSize(nativeId(id)),
    kill: async (id, options) => {
      if (options?.keepHistory) { throw unsupported('pty_kill_keep_history') }
      await native.kill(nativeId(id))
    },
    ackData: (id, _delta, processed) => fire('pty_ack_data', async () => {
      const handle = nativeId(id)
      if (processed === undefined || !Number.isSafeInteger(processed) || processed < 0) {
        throw new DesktopCommandError('pty_invalid_ack', { operation: 'pty_ack_data' },
          'Invalid cumulative terminal output acknowledgement.', 'Terminal output could not be acknowledged.')
      }
      await native.ackData(handle, processed)
    }),
    onData: (listener) => subscribe(native.onData, (payload) => {
      for (const chunk of payload.chunks) { listener({ id: String(payload.id), ...chunk }) }
      const dropped: unknown = payload.dropped
      if (dropped && typeof dropped === 'object' && 'data' in dropped && typeof dropped.data === 'string') {
        listener({ id: String(payload.id), data: dropped.data, droppedOutput: true })
      }
    }),
    onExit: (listener) => subscribe(native.onExit, (payload) => {
      listener({ id: String(payload.id), code: payload.code })
    }),
    onReplay: (listener) => subscribe(native.onReplay, (payload) => listener({ id: String(payload.id), data: payload.data })),
    onWriteUnavailable: (listener) => subscribe(native.onWriteUnavailable, listener),
    // Geometry observations cannot stand in for the unported viewport authority protocol.
    reportGeometry: noAuthority('reportGeometry'),
    claimViewport: noAuthority('claimViewport'),
    signal: noAuthority('signal'),
    clearBuffer: noAuthority('clearBuffer'),
    ackColdRestore: noAuthority('ackColdRestore'),
    rendererDispatcherReady: noAuthority('rendererDispatcherReady'),
    setActiveRendererPty: noAuthority('setActiveRendererPty'),
    setRendererPtyVisible: noAuthority('setRendererPtyVisible'),
    setHiddenRendererPty: noAuthority('setHiddenRendererPty'),
    setPtyDeliveryInterest: noAuthority('setPtyDeliveryInterest'),
    publishTerminalViewAttributes: noAuthority('publishTerminalViewAttributes'),
    sendSerializedBuffer: noAuthority('sendSerializedBuffer'),
    respondDeliveryResync: noAuthority('respondDeliveryResync'),
    spawn: () => reject('pty_spawn_preload_contract'),
    getForegroundProcess: () => reject('getForegroundProcess'),
    confirmForegroundProcess: () => reject('confirmForegroundProcess'),
    inspectProcess: () => reject('inspectProcess'),
    hasChildProcesses: () => reject('hasChildProcesses'),
    getCwd: () => reject('getCwd'),
    listSessions: () => reject('listSessions'),
    getMainBufferSnapshot: () => reject('getMainBufferSnapshot'),
    getSideEffectSnapshot: () => reject('getSideEffectSnapshot'),
    getCodexSessionOptions: () => reject('getCodexSessionOptions'),
    setCodexSessionOptions: () => reject('setCodexSessionOptions'),
    getRendererDeliveryDebugSnapshot: () => reject('getRendererDeliveryDebugSnapshot'),
    resetRendererDeliveryDebug: () => reject('resetRendererDeliveryDebug'),
    declarePendingPaneSerializer: () => reject('declarePendingPaneSerializer'),
    settlePaneSerializer: () => reject('settlePaneSerializer'),
    clearPendingPaneSerializer: () => reject('clearPendingPaneSerializer'),
    onSerializeBufferRequest: noEvents('onSerializeBufferRequest'),
    onClearBufferRequest: noEvents('onClearBufferRequest'),
    onModelRestoreNeeded: noEvents('onModelRestoreNeeded'),
    onSideEffect: noEvents('onSideEffect'),
    onSpawned: noEvents('onSpawned')
    // Unported capability and delivery-health probes stay absent so guards remain meaningful.
  } satisfies Partial<PtyApi>
}
