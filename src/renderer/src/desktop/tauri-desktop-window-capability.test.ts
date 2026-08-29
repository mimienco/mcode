import { expect, it, vi } from 'vitest'
import { createTauriDesktopWindowCapability } from './tauri-desktop-window-capability'

it('prevents a native close request and forwards it to the typed close coordinator', async () => {
  let onCloseRequested: ((event: { preventDefault: () => void }) => void) | undefined
  const preventDefault = vi.fn()
  const listener = vi.fn()
  const capability = createTauriDesktopWindowCapability({
    minimize: () => Promise.resolve(),
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onResized: () => Promise.resolve(() => undefined),
    close: () => Promise.resolve(),
    onCloseRequested: (handler) => {
      onCloseRequested = handler
      return Promise.resolve(() => undefined)
    }
  })

  await capability.onCloseRequested(listener)
  onCloseRequested?.({ preventDefault })

  expect(preventDefault).toHaveBeenCalledOnce()
  expect(listener).toHaveBeenCalledWith({ isQuitting: false })
})

it('lets exactly one confirmed close pass without re-entering the coordinator', async () => {
  let onCloseRequested: ((event: { preventDefault: () => void }) => void) | undefined
  const preventDefault = vi.fn()
  const listener = vi.fn()
  const source = {
    minimize: () => Promise.resolve(),
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onResized: () => Promise.resolve(() => undefined),
    close: vi.fn(async () => onCloseRequested?.({ preventDefault })),
    onCloseRequested: (
      handler: (event: { preventDefault: () => void }) => void
    ): Promise<() => void> => {
      onCloseRequested = handler
      return Promise.resolve(() => undefined)
    }
  }
  const capability = createTauriDesktopWindowCapability(source)
  await capability.onCloseRequested(listener)

  await capability.confirmClose()

  expect(source.close).toHaveBeenCalledOnce()
  expect(preventDefault).not.toHaveBeenCalled()
  expect(listener).not.toHaveBeenCalled()
})
