import { isTauri } from '@tauri-apps/api/core'
import type { DragDropEvent } from '@tauri-apps/api/webviewWindow'
import {
  createNativeFileDropPayload,
  resolveNativeFileDropPath,
  type NativeFileDropPathEntry,
  type NativeFileDropPayload
} from '../../../shared/native-file-drop'

type FileDropListener = (payload: NativeFileDropPayload) => void
type TauriDropSource = {
  onDragDropEvent: (listener: (event: { payload: DragDropEvent }) => void) => Promise<() => void>
}

type RuntimeDesktopFileDropDependencies = {
  isTauri: () => boolean
  loadTauriDropSource: () => Promise<TauriDropSource>
  onElectronFileDrop: (listener: FileDropListener) => () => void
  resolveElementFromPoint: (x: number, y: number) => Element | null
  scaleFactor: () => number
  reportSubscriptionError: (error: unknown) => void
}

export type RuntimeDesktopFileDropClient = {
  onFileDrop: (listener: FileDropListener) => () => void
}

function getDropPathEntries(element: Element | null): NativeFileDropPathEntry[] {
  const entries: NativeFileDropPathEntry[] = []
  let current = element
  while (current) {
    if (current instanceof HTMLElement) {
      entries.push({
        nativeFileDropTarget: current.dataset.nativeFileDropTarget,
        nativeFileDropDir: current.dataset.nativeFileDropDir,
        terminalTabId: current.dataset.terminalTabId,
        terminalPaneLeafId: current.dataset.terminalPaneLeafId ?? current.dataset.leafId
      })
    }
    if (current.parentElement) {
      current = current.parentElement
      continue
    }
    const root = current.getRootNode()
    current = root instanceof ShadowRoot ? root.host : null
  }
  return entries
}

function toFileDropPayload(
  event: DragDropEvent,
  dependencies: RuntimeDesktopFileDropDependencies
): NativeFileDropPayload | null {
  if (event.type !== 'drop' || event.paths.length === 0) {
    return null
  }
  const requestedScaleFactor = dependencies.scaleFactor()
  const scaleFactor =
    Number.isFinite(requestedScaleFactor) && requestedScaleFactor > 0 ? requestedScaleFactor : 1
  const element = dependencies.resolveElementFromPoint(
    event.position.x / scaleFactor,
    event.position.y / scaleFactor
  )
  const resolution = resolveNativeFileDropPath(getDropPathEntries(element))
  return createNativeFileDropPayload(resolution, event.paths)
}

export function createRuntimeDesktopFileDropClient(
  dependencies: RuntimeDesktopFileDropDependencies
): RuntimeDesktopFileDropClient {
  return {
    onFileDrop(listener) {
      if (!dependencies.isTauri()) {
        return dependencies.onElectronFileDrop(listener)
      }
      let disposed = false
      let nativeUnlisten: (() => void) | undefined
      void dependencies
        .loadTauriDropSource()
        .then((source) =>
          source.onDragDropEvent((event) => {
            if (disposed) {
              return
            }
            const payload = toFileDropPayload(event.payload, dependencies)
            if (payload) {
              listener(payload)
            }
          })
        )
        .then((unlisten) => {
          if (disposed) {
            unlisten()
            return
          }
          nativeUnlisten = unlisten
        })
        .catch((error: unknown) => {
          if (!disposed) {
            dependencies.reportSubscriptionError(error)
          }
        })
      return () => {
        if (disposed) {
          return
        }
        disposed = true
        nativeUnlisten?.()
        nativeUnlisten = undefined
      }
    }
  }
}

export const runtimeDesktopFileDropClient = createRuntimeDesktopFileDropClient({
  isTauri,
  loadTauriDropSource: async () =>
    (await import('@tauri-apps/api/webviewWindow')).getCurrentWebviewWindow(),
  onElectronFileDrop: (listener) => window.api.ui.onFileDrop(listener),
  resolveElementFromPoint: (x, y) => document.elementFromPoint(x, y),
  scaleFactor: () => window.devicePixelRatio,
  reportSubscriptionError: (error) => {
    console.error('[file-drop] failed to subscribe to native drag-drop events', error)
  }
})
