import type { PtyDataPayload } from './pty-client'

export function createPtySmokeOutputTracker(echo: string) {
  const sessions = new Map<number, { processedChars: number; tail: string; sawEcho: boolean }>()
  return (payload: PtyDataPayload): { processedChars: number; sawEcho: boolean } => {
    const state = sessions.get(payload.id) ?? { processedChars: 0, tail: '', sawEcho: false }
    const data = (payload.dropped?.data ?? '') + payload.chunks.map((chunk) => chunk.data).join('')
    state.processedChars += data.length
    const combined = state.tail + data
    state.sawEcho ||= combined.includes(echo)
    state.tail = combined.slice(-Math.max(0, echo.length - 1))
    sessions.set(payload.id, state)
    return { processedChars: state.processedChars, sawEcho: state.sawEcho }
  }
}
