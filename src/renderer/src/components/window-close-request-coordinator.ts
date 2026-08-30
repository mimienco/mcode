import { toast } from 'sonner'
import { translate } from '@/i18n/i18n'
import { MCODE_RENDERER_SHUTDOWN_CHECKPOINT_FAILED_EVENT } from '../../../shared/renderer-shutdown-events'
import { MCODE_APP_RESTART_ABORTED_EVENT } from '../../../shared/updater-renderer-events'
import type { DesktopWindowCapability, DesktopWindowCloseRequest } from '../desktop/desktop-client'
import { desktopWindowClient } from '../desktop/runtime-desktop-window-client'

// Coordinates the single main->renderer window-close-request subscription (owned
// by the always-mounted App root) with the rich close-confirmation handler in
// Terminal, which only mounts once a workspace exists. Without this, quitting on
// the no-workspace landing page — where Terminal (and its listener) is not
// mounted — sends 'window:close-requested' to a renderer with no handler, so
// confirmWindowClose() is never called and the window never closes (#5144).
//
// It also runs pre-close guards: surfaces with unsaved work (e.g. the Settings
// Git AI Author prompt editors) register a guard so quitting prompts the user to
// save/discard instead of being silently vetoed by a beforeunload handler.

export type WindowCloseRequestHandler = (data: DesktopWindowCloseRequest) => void

/** Returns true to allow the close to proceed, false to cancel it (e.g. the user
 *  picked "Cancel" in an unsaved-changes prompt). */
export type WindowCloseGuard = () => boolean | Promise<boolean>
export type WindowCloseFinalizer = () => void | Promise<void>

let activeHandler: WindowCloseRequestHandler | null = null
const closeGuards = new Set<WindowCloseGuard>()
const closeFinalizers = new Set<WindowCloseFinalizer>()
// Why: a guard can await a dialog; ignore re-entrant close requests (main resends
// 'window:close-requested' on each attempt) so we don't stack duplicate prompts.
let closeInFlight = false

/** Terminal registers its rich handler while mounted; passing null on unmount
 *  hands the decision back to the App-root fallback. */
export function setWindowCloseRequestHandler(handler: WindowCloseRequestHandler | null): void {
  activeHandler = handler
}

export function getWindowCloseRequestHandler(): WindowCloseRequestHandler | null {
  return activeHandler
}

/** Register a pre-close guard. Returns an unregister function for effect cleanup. */
export function registerWindowCloseGuard(guard: WindowCloseGuard): () => void {
  closeGuards.add(guard)
  return () => {
    closeGuards.delete(guard)
  }
}

export function registerWindowCloseFinalizer(finalizer: WindowCloseFinalizer): () => void {
  closeFinalizers.add(finalizer)
  return () => {
    closeFinalizers.delete(finalizer)
  }
}

async function runWindowCloseGuards(): Promise<boolean> {
  for (const guard of closeGuards) {
    if (!(await guard())) {
      return false
    }
  }
  return true
}

type WindowCloseDecisionCapability = Pick<DesktopWindowCapability, 'cancelClose' | 'confirmClose'>

export async function cancelPendingWindowClose(
  desktopWindow: Pick<DesktopWindowCapability, 'cancelClose'> = desktopWindowClient
): Promise<void> {
  try {
    const request = await desktopWindow.cancelClose()
    if (
      (request?.intent === 'restart' || request?.intent === 'update_install') &&
      typeof window.dispatchEvent === 'function'
    ) {
      window.dispatchEvent(new Event(MCODE_APP_RESTART_ABORTED_EVENT))
    }
  } catch (error) {
    console.error('[app] Failed to cancel native shutdown request', error)
  }
}

export async function finalizeAndConfirmWindowClose(
  desktopWindow: WindowCloseDecisionCapability = desktopWindowClient
): Promise<boolean> {
  try {
    for (const finalizer of closeFinalizers) {
      await finalizer()
    }
  } catch (error) {
    await cancelPendingWindowClose(desktopWindow)
    console.error('[app] Final window-close durability work failed', error)
    toast.error(
      translate('auto.App.shutdownCheckpointFailed', 'MCode could not save your session'),
      {
        description: translate(
          'auto.App.shutdownCheckpointFailedDescription',
          'The window stayed open. Try closing it again.'
        )
      }
    )
    if (typeof window.dispatchEvent === 'function') {
      window.dispatchEvent(new Event(MCODE_RENDERER_SHUTDOWN_CHECKPOINT_FAILED_EVENT))
    }
    return false
  }
  try {
    await desktopWindow.confirmClose()
  } catch (error) {
    await cancelPendingWindowClose(desktopWindow)
    console.error('[app] Native window close confirmation failed', error)
    toast.error(translate('auto.App.windowCloseFailed', 'MCode could not close this window'), {
      description: translate(
        'auto.App.windowCloseFailedDescription',
        'The window stayed open. Try closing it again.'
      )
    })
    return false
  }
  return true
}

/** Route a main-process close request: run pre-close guards first (cancel if any
 *  vetoes), then delegate to Terminal's rich handler when mounted, else confirm
 *  directly. Why confirm directly: with no workbench mounted there are no
 *  terminals or editor tabs to protect, so blocking would just deadlock the
 *  window (#5144). */
export async function dispatchWindowCloseRequest(
  data: DesktopWindowCloseRequest,
  desktopWindow: WindowCloseDecisionCapability = desktopWindowClient
): Promise<void> {
  if (closeInFlight) {
    return
  }
  closeInFlight = true
  try {
    if (!(await runWindowCloseGuards())) {
      await cancelPendingWindowClose(desktopWindow)
      return
    }
  } finally {
    closeInFlight = false
  }
  if (activeHandler) {
    activeHandler(data)
    return
  }
  await finalizeAndConfirmWindowClose(desktopWindow)
}
