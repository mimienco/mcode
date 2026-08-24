// Why: PERF-12 — every terminal pane used to pin its own global
// pointerdown(capture)/window blur/focus listeners. Handlers are idempotent for
// non-owners (see regular-terminal-focus-ownership), so one shared listener set
// fanning out to registered panes is observably identical with O(1) listeners.

export type WindowTerminalFocusParticipant = {
  onPointerDown: (event: PointerEvent) => void
  onWindowBlur: () => void
  onWindowFocus: () => boolean | void
}

export type WindowTerminalFocusCoordinator = {
  acquire: (participant: WindowTerminalFocusParticipant) => () => void
  participantCount: () => number
}

export function createWindowTerminalFocusCoordinator(
  target: { document: Document; window: Window & typeof globalThis } = {
    document: document as Document,
    window: window as Window & typeof globalThis
  }
): WindowTerminalFocusCoordinator {
  const participants = new Map<symbol, WindowTerminalFocusParticipant>()
  let installed = false

  const handlePointerDown = (event: PointerEvent): void => {
    for (const participant of participants.values()) {
      participant.onPointerDown(event)
    }
  }
  const handleWindowBlur = (): void => {
    for (const participant of participants.values()) {
      participant.onWindowBlur()
    }
  }
  const handleWindowFocus = (): void => {
    for (const participant of participants.values()) {
      participant.onWindowFocus()
    }
  }

  const install = (): void => {
    if (installed) {
      return
    }
    installed = true
    target.document.addEventListener('pointerdown', handlePointerDown, true)
    target.window.addEventListener('blur', handleWindowBlur)
    target.window.addEventListener('focus', handleWindowFocus)
  }

  const uninstall = (): void => {
    if (participants.size > 0 || !installed) {
      return
    }
    installed = false
    target.document.removeEventListener('pointerdown', handlePointerDown, true)
    target.window.removeEventListener('blur', handleWindowBlur)
    target.window.removeEventListener('focus', handleWindowFocus)
  }

  return {
    acquire: (participant) => {
      install()
      const key = Symbol('terminal-focus-participant')
      participants.set(key, participant)
      return () => {
        participants.delete(key)
        uninstall()
      }
    },
    participantCount: () => participants.size
  }
}

/** Process-wide singleton: terminal panes are module-level peers of this coordinator. */
export const windowTerminalFocusCoordinator = createWindowTerminalFocusCoordinator()
