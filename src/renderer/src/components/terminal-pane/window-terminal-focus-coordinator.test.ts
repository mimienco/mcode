// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from 'vitest'
import { createWindowTerminalFocusCoordinator } from './window-terminal-focus-coordinator'

// PERF-12: N terminal panes must share ONE set of global focus listeners.
// Dispatch order follows registration and every participant receives every
// event; ownership filtering happens inside each pane's handlers.
function makeParticipant() {
  return {
    onPointerDown: vi.fn(),
    onWindowBlur: vi.fn(),
    onWindowFocus: vi.fn(() => false)
  }
}

describe('window terminal focus coordinator', () => {
  const addDocSpy = vi.spyOn(document, 'addEventListener')
  const removeDocSpy = vi.spyOn(document, 'removeEventListener')
  const addWinSpy = vi.spyOn(window, 'addEventListener')
  const removeWinSpy = vi.spyOn(window, 'removeEventListener')

  afterEach(() => {
    addDocSpy.mockClear()
    removeDocSpy.mockClear()
    addWinSpy.mockClear()
    removeWinSpy.mockClear()
  })

  it('installs exactly one listener triple for many panes and removes them when idle', () => {
    const coordinator = createWindowTerminalFocusCoordinator()
    const releases = Array.from({ length: 20 }, () => coordinator.acquire(makeParticipant()))

    const docAdds = addDocSpy.mock.calls.filter(([type]) => type === 'pointerdown').length
    const blurAdds = addWinSpy.mock.calls.filter(([type]) => type === 'blur').length
    const focusAdds = addWinSpy.mock.calls.filter(([type]) => type === 'focus').length
    expect(docAdds).toBe(1)
    expect(blurAdds).toBe(1)
    expect(focusAdds).toBe(1)
    expect(coordinator.participantCount()).toBe(20)

    for (const release of releases) {
      release()
    }
    const docRemoves = removeDocSpy.mock.calls.filter(([type]) => type === 'pointerdown').length
    const blurRemoves = removeWinSpy.mock.calls.filter(([type]) => type === 'blur').length
    const focusRemoves = removeWinSpy.mock.calls.filter(([type]) => type === 'focus').length
    expect(docRemoves).toBe(1)
    expect(blurRemoves).toBe(1)
    expect(focusRemoves).toBe(1)
    expect(coordinator.participantCount()).toBe(0)
  })

  it('fans pointerdown out to every registered pane', () => {
    const coordinator = createWindowTerminalFocusCoordinator()
    const a = makeParticipant()
    const b = makeParticipant()
    coordinator.acquire(a)
    coordinator.acquire(b)

    const event = new PointerEvent('pointerdown')
    document.dispatchEvent(event)

    expect(a.onPointerDown).toHaveBeenCalledTimes(1)
    expect(b.onPointerDown).toHaveBeenCalledTimes(1)
    expect(a.onPointerDown.mock.calls[0][0]).toBe(event)
  })

  it('keeps dispatching to remaining panes after another pane releases mid-flight', () => {
    const coordinator = createWindowTerminalFocusCoordinator()
    const a = makeParticipant()
    const b = makeParticipant()
    const releaseA = coordinator.acquire(a)
    coordinator.acquire(b)
    releaseA()

    window.dispatchEvent(new Event('blur'))
    window.dispatchEvent(new Event('focus'))

    expect(a.onWindowBlur).not.toHaveBeenCalled()
    expect(a.onWindowFocus).not.toHaveBeenCalled()
    expect(b.onWindowBlur).toHaveBeenCalledTimes(1)
    expect(b.onWindowFocus).toHaveBeenCalledTimes(1)
  })

  it('propagates the window-focus reclaim result from participants', () => {
    const coordinator = createWindowTerminalFocusCoordinator()
    const reclaiming = makeParticipant()
    reclaiming.onWindowFocus = vi.fn(() => true)
    coordinator.acquire(reclaiming)

    // Why: no throw even though the participant reports success — the latch
    // reset lives in the pane closure, not the coordinator.
    expect(() => window.dispatchEvent(new Event('focus'))).not.toThrow()
    expect(reclaiming.onWindowFocus).toHaveBeenCalledTimes(1)
  })
})
