//! Seq-dedupe for the Tauri PTY transport (TERMINAL_UI_INTEGRATION.md §2.3).

import type { PtyApi, PtyDataChunk, PtyDataPayload } from '../../desktop/pty-client'
import { getProcessedPtyCharTotals } from './terminal-pty-ack-gate'
import { setPtyDispatcherEventPort } from './pty-dispatcher'

// Why module-level: the dispatcher port is a page-wide seam shared by every
// Tauri transport; the restore-marker baseline is per-PTY-id accounting that
// must survive a transport rebind for a live id, exactly like the cumulative
// totals it complements. Entries clear on pty exit.
export const restoreMarkerBaselines = new Map<string, number>()

export function installDispatcherEventPort(instanceClient: PtyApi): void {
  setPtyDispatcherEventPort({
    onData: (listener) =>
      instanceClient.onData((payload) => {
        const id = String(payload.id)
        // Why: an in-band restore marker resets the dedupe window to markerSeq
        // (spec §2.3) — it must not be vetoed by pre-marker processed totals,
        // which include bytes only recoverable from the model snapshot.
        const baseline = restoreMarkerBaselines.get(id) ?? getProcessedPtyCharTotals()[id] ?? 0
        const deduped = applyPtySeqDedupe(payload, baseline)
        if (payload.restoreMarker) {
          restoreMarkerBaselines.set(id, deduped.nextBaseline)
        }
        for (const chunk of deduped.chunks) {
          listener({
            id,
            data: chunk.data,
            seq: chunk.seq,
            ...(chunk.rawLength !== undefined ? { rawLength: chunk.rawLength } : {}),
            ...(chunk.transformed !== undefined ? { transformed: chunk.transformed } : {})
          })
        }
      }),
    onExit: (listener) =>
      instanceClient.onExit((payload) => {
        restoreMarkerBaselines.delete(String(payload.id))
        // No preserveRendererBinding under Tauri — remote-sleep machinery is
        // Electron-only; the dispatcher guard treats its absence as false.
        listener({ id: String(payload.id), code: payload.code })
      })
  })
}

/**
 * Applies the §2.3 seq-dedupe rule to one `PtyDataPayload` against the
 * renderer's cumulative-ACK baseline: a chunk whose covered span
 * (`seq - (rawLength ?? data.length)`) starts before the baseline is already
 * processed and skipped; a chunk starting exactly at the baseline is kept.
 * An in-band `restoreMarker` resets the dedupe window to `markerSeq` —
 * everything at or below it is only recoverable from the model snapshot —
 * for this payload's chunks and every subsequent one.
 */
export function applyPtySeqDedupe(
  payload: Pick<PtyDataPayload, 'chunks' | 'restoreMarker'>,
  baseline: number
): { chunks: PtyDataChunk[]; nextBaseline: number } {
  const effectiveBaseline = payload.restoreMarker?.markerSeq ?? baseline
  return {
    chunks: payload.chunks.filter(
      (chunk) => chunk.seq - (chunk.rawLength ?? chunk.data.length) >= effectiveBaseline
    ),
    nextBaseline: effectiveBaseline
  }
}
