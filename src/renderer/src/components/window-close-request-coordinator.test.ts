import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  cancelPendingWindowClose,
  dispatchWindowCloseRequest,
  getWindowCloseRequestHandler,
  registerWindowCloseFinalizer,
  registerWindowCloseGuard,
  setWindowCloseRequestHandler
} from './window-close-request-coordinator'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { MCODE_APP_RESTART_ABORTED_EVENT } from '../../../shared/updater-renderer-events'

const toastError = vi.hoisted(() => vi.fn())

vi.mock('sonner', () => ({ toast: { error: toastError } }))

describe('window-close-request-coordinator', () => {
  const confirmWindowClose = vi.fn()
  const dispatchEvent = vi.fn()
  const unregisterFns: (() => void)[] = []

  const addGuard = (guard: () => boolean | Promise<boolean>): void => {
    unregisterFns.push(registerWindowCloseGuard(guard))
  }

  beforeEach(() => {
    confirmWindowClose.mockClear()
    dispatchEvent.mockClear()
    toastError.mockClear()
    // Why: dispatch falls back to the preload bridge when no rich handler is
    // registered; stub just the surface it touches.
    ;(
      globalThis as unknown as {
        window: {
          api: { ui: { confirmWindowClose: () => void } }
          dispatchEvent: typeof dispatchEvent
        }
      }
    ).window = { api: { ui: { confirmWindowClose } }, dispatchEvent }
  })

  afterEach(() => {
    setWindowCloseRequestHandler(null)
    unregisterFns.splice(0).forEach((fn) => fn())
  })

  it('has no handler by default, so the App root falls back to confirming the close', () => {
    // Why: on the no-workspace landing page Terminal is not mounted, so no rich
    // handler is registered and the App-root subscription must close directly.
    expect(getWindowCloseRequestHandler()).toBeNull()
  })

  it('returns the registered handler so the App root delegates to Terminal', () => {
    const handler = vi.fn()
    setWindowCloseRequestHandler(handler)
    expect(getWindowCloseRequestHandler()).toBe(handler)
  })

  it('clears the handler on unmount so a stale Terminal closure cannot run', () => {
    setWindowCloseRequestHandler(vi.fn())
    setWindowCloseRequestHandler(null)
    expect(getWindowCloseRequestHandler()).toBeNull()
  })

  // The #5144 contract: a close request must always be acted on.
  it('confirms the close directly when no rich handler is registered (no-workspace path)', async () => {
    await dispatchWindowCloseRequest({ isQuitting: true })

    expect(confirmWindowClose).toHaveBeenCalledTimes(1)
  })

  it('confirms the landing-page close through the typed desktop capability', async () => {
    const confirmClose = vi.fn(() => Promise.resolve())

    await dispatchWindowCloseRequest({ isQuitting: false }, { confirmClose } as never)

    expect(confirmClose).toHaveBeenCalledOnce()
    expect(confirmWindowClose).not.toHaveBeenCalled()
  })

  it('delegates to the rich handler and does NOT confirm directly when one is registered', async () => {
    const handler = vi.fn()
    setWindowCloseRequestHandler(handler)

    await dispatchWindowCloseRequest({ isQuitting: false })

    expect(handler).toHaveBeenCalledWith({ isQuitting: false })
    // Why: confirmation is the rich handler's responsibility (after save dialogs
    // / running-process checks) — dispatch must not short-circuit it.
    expect(confirmWindowClose).not.toHaveBeenCalled()
  })

  // Pre-close guards (e.g. unsaved Settings prompt drafts).
  it('cancels the close — no confirm, no handler — when a guard vetoes', async () => {
    const handler = vi.fn()
    setWindowCloseRequestHandler(handler)
    addGuard(() => false)

    await dispatchWindowCloseRequest({ isQuitting: true })

    expect(confirmWindowClose).not.toHaveBeenCalled()
    expect(handler).not.toHaveBeenCalled()
  })

  it('proceeds to confirm when all guards allow the close', async () => {
    addGuard(() => true)
    addGuard(async () => true)

    await dispatchWindowCloseRequest({ isQuitting: true })

    expect(confirmWindowClose).toHaveBeenCalledTimes(1)
  })

  it('awaits final durability work immediately before confirming a landing-page close', async () => {
    const calls: string[] = []
    unregisterFns.push(
      registerWindowCloseFinalizer(async () => {
        calls.push('durable')
      })
    )

    await dispatchWindowCloseRequest({ isQuitting: false }, {
      async confirmClose() {
        calls.push('confirm')
      }
    } as never)

    expect(calls).toEqual(['durable', 'confirm'])
  })

  it('keeps the window open when final durability work rejects', async () => {
    const confirmClose = vi.fn(() => Promise.resolve())
    unregisterFns.push(registerWindowCloseFinalizer(() => Promise.reject(new Error('disk full'))))

    await dispatchWindowCloseRequest({ isQuitting: false }, { confirmClose } as never)

    expect(confirmClose).not.toHaveBeenCalled()
    expect(toastError).toHaveBeenCalledWith(
      'MCode could not save your session',
      expect.objectContaining({
        description: 'The window stayed open. Try closing it again.'
      })
    )
    expect(dispatchEvent).toHaveBeenCalledOnce()
  })

  it('cancels a pending native restart when final durability work rejects', async () => {
    const confirmClose = vi.fn(() => Promise.resolve())
    const cancelClose = vi.fn(() =>
      Promise.resolve({ version: 1 as const, requestId: '15', intent: 'restart' as const })
    )
    unregisterFns.push(registerWindowCloseFinalizer(() => Promise.reject(new Error('disk full'))))

    await dispatchWindowCloseRequest({ isQuitting: true }, { confirmClose, cancelClose })

    expect(cancelClose).toHaveBeenCalledOnce()
    expect(confirmClose).not.toHaveBeenCalled()
    expect(dispatchEvent.mock.calls.map(([event]) => event.type)).toContain(
      MCODE_APP_RESTART_ABORTED_EVENT
    )
  })

  it('cancels the exact native request when a deferred dirty-file decision is vetoed', async () => {
    const cancelClose = vi.fn(() =>
      Promise.resolve({ version: 1 as const, requestId: '16', intent: 'quit' as const })
    )

    await cancelPendingWindowClose({ cancelClose })

    expect(cancelClose).toHaveBeenCalledOnce()
  })

  it('routes the dirty-file Cancel action back to native shutdown cancellation', () => {
    const source = readFileSync(resolve('src/renderer/src/components/Terminal.tsx'), 'utf8')
    const cancelStart = source.indexOf('const handleSaveDialogCancel = useCallback')
    const cancelEnd = source.indexOf('useEffect(() => {', cancelStart)
    const cancelHandler = source.slice(cancelStart, cancelEnd)

    expect(cancelStart).toBeGreaterThanOrEqual(0)
    expect(cancelHandler).toContain('windowCloseAfterDirtyRef.current !== null')
    expect(cancelHandler).toContain('void cancelPendingWindowClose()')
  })

  it('keeps the window open and reports a native close failure', async () => {
    const confirmClose = vi.fn(() => Promise.reject(new Error('native close failed')))

    await dispatchWindowCloseRequest({ isQuitting: false }, { confirmClose } as never)

    expect(toastError).toHaveBeenCalledWith(
      'MCode could not close this window',
      expect.objectContaining({
        description: 'The window stayed open. Try closing it again.'
      })
    )
    expect(dispatchEvent).not.toHaveBeenCalled()
  })

  it('short-circuits on the first vetoing guard', async () => {
    const second = vi.fn(() => true)
    addGuard(() => false)
    addGuard(second)

    await dispatchWindowCloseRequest({ isQuitting: true })

    expect(second).not.toHaveBeenCalled()
    expect(confirmWindowClose).not.toHaveBeenCalled()
  })

  it('ignores a re-entrant close request while a guard is still pending', async () => {
    let resolveGuard: (value: boolean) => void = () => {}
    const guard = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          resolveGuard = resolve
        })
    )
    addGuard(guard)

    const first = dispatchWindowCloseRequest({ isQuitting: true })
    // Second request arrives while the first guard's dialog is still open.
    await dispatchWindowCloseRequest({ isQuitting: true })
    expect(guard).toHaveBeenCalledTimes(1)

    resolveGuard(true)
    await first
    expect(confirmWindowClose).toHaveBeenCalledTimes(1)
  })

  it('stops consulting a guard once it is unregistered', async () => {
    const guard = vi.fn(() => false)
    const unregister = registerWindowCloseGuard(guard)
    unregister()

    await dispatchWindowCloseRequest({ isQuitting: true })

    expect(guard).not.toHaveBeenCalled()
    expect(confirmWindowClose).toHaveBeenCalledTimes(1)
  })
})
