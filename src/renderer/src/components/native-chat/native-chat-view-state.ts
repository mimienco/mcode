// Pure mapping from an assembled NativeChatSession to the discrete view state the
// UI renders. Keeping it a single function (not branching inside the .tsx) makes
// the empty/loading/error/working/ready dispatch testable and keeps the render
// tree to one switch.

import type { NativeChatSession } from '../../../../shared/native-chat-types'

/** The mutually-exclusive surfaces the chat view can show. `ready` and
 *  `working` both render the message list; `working` additionally shows the
 *  live in-flight indicator. The rest are full-pane states. */
export type NativeChatViewState =
  | { kind: 'loading' }
  | { kind: 'error'; message: string }
  | { kind: 'empty' }
  | { kind: 'ready'; isWorking: false }
  | { kind: 'ready'; isWorking: true }

/**
 * Decide which surface to render. Error is terminal, but any renderable message
 * wins over loading/empty so optimistic first sends never get replaced by a
 * full-pane placeholder while transcript discovery catches up.
 */
export function selectNativeChatViewState(
  session: NativeChatSession,
  options?: { transcriptSettled?: boolean }
): NativeChatViewState {
  if (session.status === 'error') {
    return { kind: 'error', message: session.error ?? 'Conversation could not be loaded.' }
  }
  if (session.messages.length > 0) {
    return { kind: 'ready', isWorking: session.status === 'working' }
  }
  if (session.status === 'loading' && options?.transcriptSettled !== true) {
    return { kind: 'loading' }
  }
  // A known session working with nothing to show may be waiting on its first
  // transcript flush, so hold loading only until the transcript read settles.
  // The status stays 'working', so the composer keeps Stop the moment a bubble
  // lands — forcing 'loading' upstream instead rendered an idle pane mid-turn.
  if (
    session.status === 'working' &&
    session.sessionId !== null &&
    options?.transcriptSettled !== true
  ) {
    return { kind: 'loading' }
  }
  // Empty wins over a transient 'working' hook so a just-toggled, pre-session
  // pane shows a clear empty state instead of a spinner over nothing.
  return { kind: 'empty' }
}
