import { resetAndRefreshAllTerminalWebglAtlases } from '@/lib/pane-manager/pane-manager-registry'

const ATLAS_RECOVERY_DELAYS_MS = [120, 500]
let tabRevealFramePending = false
const tabRevealTimers = new Map<number, ReturnType<typeof setTimeout>>()

function scheduleNextFrame(callback: () => void): void {
  if (typeof globalThis.requestAnimationFrame === 'function') {
    globalThis.requestAnimationFrame(callback)
    return
  }
  globalThis.setTimeout(callback, 0)
}

function resetAtlasesAndRefreshPanes(reason: string): void {
  try {
    // Why: the glyph atlas is shared across same-config terminals, so the
    // recovery reset must be followed by repainting each rebuilt render model.
    resetAndRefreshAllTerminalWebglAtlases(reason)
  } catch {
    /* ignore - terminal pane may have unmounted after scheduling recovery */
  }
}

function scheduleAtlasRecoveryBurst(reason: string): void {
  scheduleNextFrame(() => resetAtlasesAndRefreshPanes(reason))
  for (const delayMs of ATLAS_RECOVERY_DELAYS_MS) {
    globalThis.setTimeout(() => resetAtlasesAndRefreshPanes(reason), delayMs)
  }
}

export function scheduleImagePasteWebglAtlasRecovery(): void {
  // Why: image chips can redraw after bracketed paste parsing, so cover the
  // short post-paste paint window with a few cheap atlas rebuilds. Paste is a
  // one-shot event, so recover immediately rather than debouncing.
  scheduleAtlasRecoveryBurst('image-paste')
}

export function scheduleTabRevealWebglAtlasRecovery(): void {
  if (!tabRevealFramePending) {
    tabRevealFramePending = true
    scheduleNextFrame(() => {
      tabRevealFramePending = false
      resetAtlasesAndRefreshPanes('tab-reveal')
    })
  }
  for (const delayMs of ATLAS_RECOVERY_DELAYS_MS) {
    if (tabRevealTimers.has(delayMs)) {
      continue
    }
    const timerId = globalThis.setTimeout(() => {
      tabRevealTimers.delete(delayMs)
      resetAtlasesAndRefreshPanes('tab-reveal')
    }, delayMs)
    tabRevealTimers.set(delayMs, timerId)
  }
}

export function resetTerminalWebglAtlasRecoveryForTest(): void {
  tabRevealFramePending = false
  for (const timerId of tabRevealTimers.values()) {
    globalThis.clearTimeout(timerId)
  }
  tabRevealTimers.clear()
}
