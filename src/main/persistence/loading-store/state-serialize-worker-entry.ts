import { parentPort } from 'node:worker_threads'
import { splitSerializedAtSentinels } from './state-secret-substitution'

// Why: PERF-08 commit 2b — the sentinel SCAN over multi-MB serialized state
// runs here instead of blocking the Electron main thread. Only positional data
// crosses this boundary: the worker never receives secret material (blobs and
// guard values are injected by main from the split result).

type SerializeSplitRequest = {
  id: number
  serialized: string
  /** Raw sentinels; they are JSON-escaped exactly like the legacy loop did. */
  sentinels: readonly string[]
}

type SerializeSplitResponse =
  | {
      id: number
      ok: true
      segments: string[]
      starts: number[]
    }
  | { id: number; ok: false; error: string }

parentPort?.on('message', (request: SerializeSplitRequest) => {
  let response: SerializeSplitResponse
  try {
    const { segments, starts } = splitSerializedAtSentinels(
      request.serialized,
      request.sentinels.map((sentinel) => JSON.stringify(sentinel).slice(1, -1))
    )
    response = { id: request.id, ok: true, segments, starts }
  } catch (error) {
    response = {
      id: request.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error)
    }
  }
  parentPort?.postMessage(response)
})
