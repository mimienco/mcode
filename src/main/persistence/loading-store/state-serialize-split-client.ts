import { join } from 'node:path'
import { existsSync } from 'node:fs'

// Why: PERF-08 — offload the sentinel scan of multi-MB serialized state to a
// worker thread so the Electron main loop stays responsive during debounced
// saves. Only positional data crosses postMessage; secret blobs and guard
// values are injected by main from the returned split.

export type SerializedSplit = {
  segments: string[]
  starts: number[]
}

export type SerializeSplitWorker = {
  post: (request: { id: number; serialized: string; sentinels: readonly string[] }) => void
  terminate: () => void
  onResult: (
    handler: (
      result:
        | { id: number; ok: true; segments: string[]; starts: number[] }
        | { id: number; ok: false; error: string }
    ) => void
  ) => void
}

export type SerializeSplitClientDependencies = {
  resolveWorkerPath: () => string | null
  spawnWorker: (workerPath: string) => SerializeSplitWorker | null
}

export function createSerializeSplitClient(
  dependencies: SerializeSplitClientDependencies,
  options: { timeoutMs?: number } = {}
): {
  split: (serialized: string, escapedSentinels: readonly string[]) => Promise<SerializedSplit>
} {
  const timeoutMs = options.timeoutMs ?? 1_500
  return {
    split: async (serialized, escapedSentinels) => {
      const workerPath = dependencies.resolveWorkerPath()
      if (!workerPath) {
        throw new Error('serialize worker unavailable')
      }
      const worker = dependencies.spawnWorker(workerPath)
      if (!worker) {
        throw new Error('serialize worker spawn failed')
      }
      const id = Date.now()
      return new Promise<SerializedSplit>((resolve, reject) => {
        const timer = setTimeout(() => {
          worker.terminate()
          reject(new Error('serialize worker timed out'))
        }, timeoutMs)
        worker.onResult((result) => {
          if (result.id !== id) {
            return
          }
          clearTimeout(timer)
          worker.terminate()
          if (result.ok) {
            resolve({ segments: result.segments, starts: result.starts })
          } else {
            reject(new Error(result.error))
          }
        })
        worker.post({ id, serialized, sentinels: escapedSentinels })
      })
    }
  }
}

/** Mirrors hang-watchdog's two-location resolution: adjacent build output in
 *  dev, packaged out/main otherwise. Injectable existsSync keeps tests pure. */
export function resolveStateSerializeWorkerPath(
  appPath: string,
  isPackaged: boolean,
  pathExists: (candidate: string) => boolean = existsSync
): string | null {
  // Why: rolldown nests entries by their input-key directory
  // ('persistence/...'); cover the flat legacy layout and the dev root too.
  const candidates = [
    join(appPath, 'persistence', 'state-serialize-worker-entry.js'),
    join(appPath, 'out', 'main', 'persistence', 'state-serialize-worker-entry.js'),
    join(appPath, 'out', 'main', 'state-serialize-worker-entry.js'),
    join(appPath, 'state-serialize-worker-entry.js')
  ]
  return candidates.find((candidate) => pathExists(candidate)) ?? null
}
