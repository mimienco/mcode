// @vitest-environment happy-dom

import { expect, it, vi } from 'vitest'
import type { DragDropEvent } from '@tauri-apps/api/webviewWindow'
import { createRuntimeDesktopFileDropClient } from './runtime-desktop-file-drop-client'

it('preserves the Electron native file-drop subscription contract', () => {
  const electronUnsubscribe = vi.fn()
  const onElectronFileDrop = vi.fn(() => electronUnsubscribe)
  const listener = vi.fn()
  const client = createRuntimeDesktopFileDropClient({
    isTauri: () => false,
    loadTauriDropSource: vi.fn(),
    onElectronFileDrop,
    resolveElementFromPoint: vi.fn(),
    scaleFactor: () => 1,
    reportSubscriptionError: vi.fn()
  })

  const unsubscribe = client.onFileDrop(listener)

  expect(onElectronFileDrop).toHaveBeenCalledExactlyOnceWith(listener)
  unsubscribe()
  expect(electronUnsubscribe).toHaveBeenCalledOnce()
})

it('routes a Tauri drop to the marked renderer surface using CSS coordinates', async () => {
  let tauriListener: ((event: { payload: DragDropEvent }) => void) | undefined
  const nativeUnlisten = vi.fn()
  const surface = document.createElement('section')
  surface.dataset.nativeFileDropTarget = 'file-explorer'
  surface.dataset.nativeFileDropDir = '/repo/src'
  const child = document.createElement('span')
  surface.append(child)
  const resolveElementFromPoint = vi.fn(() => child)
  const listener = vi.fn()
  const client = createRuntimeDesktopFileDropClient({
    isTauri: () => true,
    loadTauriDropSource: async () => ({
      onDragDropEvent: async (callback) => {
        tauriListener = callback
        return nativeUnlisten
      }
    }),
    onElectronFileDrop: vi.fn(),
    resolveElementFromPoint,
    scaleFactor: () => 2,
    reportSubscriptionError: vi.fn()
  })

  client.onFileDrop(listener)
  await vi.waitFor(() => expect(tauriListener).toBeTypeOf('function'))
  tauriListener?.({
    payload: {
      type: 'drop',
      paths: ['/outside/report.md'],
      position: { x: 240, y: 100 } as never
    }
  })

  expect(resolveElementFromPoint).toHaveBeenCalledExactlyOnceWith(120, 50)
  expect(listener).toHaveBeenCalledExactlyOnceWith({
    destinationDir: '/repo/src',
    paths: ['/outside/report.md'],
    target: 'file-explorer'
  })
})

it('unlistens after an async Tauri subscription resolves when already disposed', async () => {
  type DropCallback = (event: { payload: DragDropEvent }) => void
  type DropSource = { onDragDropEvent: (callback: DropCallback) => Promise<() => void> }
  let resolveSource!: (source: DropSource) => void
  let tauriListener: DropCallback | undefined
  const nativeUnlisten = vi.fn()
  const sourcePromise = new Promise<DropSource>((resolve) => {
    resolveSource = resolve
  })
  const listener = vi.fn()
  const client = createRuntimeDesktopFileDropClient({
    isTauri: () => true,
    loadTauriDropSource: () => sourcePromise,
    onElectronFileDrop: vi.fn(),
    resolveElementFromPoint: () => null,
    scaleFactor: () => 1,
    reportSubscriptionError: vi.fn()
  })

  const unsubscribe = client.onFileDrop(listener)
  unsubscribe()
  resolveSource({
    onDragDropEvent: async (callback) => {
      tauriListener = callback
      return nativeUnlisten
    }
  })

  await vi.waitFor(() => expect(nativeUnlisten).toHaveBeenCalledOnce())
  tauriListener?.({
    payload: {
      type: 'drop',
      paths: ['/outside/report.md'],
      position: { x: 1, y: 1 } as never
    }
  })
  expect(listener).not.toHaveBeenCalled()
})

it('unlistens an established Tauri subscription exactly once', async () => {
  const nativeUnlisten = vi.fn()
  const onDragDropEvent = vi.fn(async () => nativeUnlisten)
  const client = createRuntimeDesktopFileDropClient({
    isTauri: () => true,
    loadTauriDropSource: async () => ({ onDragDropEvent }),
    onElectronFileDrop: vi.fn(),
    resolveElementFromPoint: () => null,
    scaleFactor: () => 1,
    reportSubscriptionError: vi.fn()
  })

  const unsubscribe = client.onFileDrop(vi.fn())
  await vi.waitFor(() => expect(onDragDropEvent).toHaveBeenCalledOnce())
  await Promise.resolve()
  unsubscribe()
  unsubscribe()

  expect(nativeUnlisten).toHaveBeenCalledOnce()
})

it('ignores Tauri drag lifecycle events that do not contain a drop', async () => {
  let tauriListener: ((event: { payload: DragDropEvent }) => void) | undefined
  const listener = vi.fn()
  const resolveElementFromPoint = vi.fn()
  const client = createRuntimeDesktopFileDropClient({
    isTauri: () => true,
    loadTauriDropSource: async () => ({
      onDragDropEvent: async (callback) => {
        tauriListener = callback
        return vi.fn()
      }
    }),
    onElectronFileDrop: vi.fn(),
    resolveElementFromPoint,
    scaleFactor: () => 1,
    reportSubscriptionError: vi.fn()
  })

  client.onFileDrop(listener)
  await vi.waitFor(() => expect(tauriListener).toBeTypeOf('function'))
  tauriListener?.({
    payload: { type: 'enter', paths: ['/outside/report.md'], position: { x: 1, y: 1 } as never }
  })
  tauriListener?.({ payload: { type: 'leave' } })

  expect(resolveElementFromPoint).not.toHaveBeenCalled()
  expect(listener).not.toHaveBeenCalled()
})

it('reports an active Tauri subscription failure without rejecting cleanup', async () => {
  const error = new Error('native listener unavailable')
  const reportSubscriptionError = vi.fn()
  const client = createRuntimeDesktopFileDropClient({
    isTauri: () => true,
    loadTauriDropSource: () => Promise.reject(error),
    onElectronFileDrop: vi.fn(),
    resolveElementFromPoint: vi.fn(),
    scaleFactor: () => 1,
    reportSubscriptionError
  })

  const unsubscribe = client.onFileDrop(vi.fn())
  await vi.waitFor(() => expect(reportSubscriptionError).toHaveBeenCalledExactlyOnceWith(error))
  expect(unsubscribe).not.toThrow()
})
