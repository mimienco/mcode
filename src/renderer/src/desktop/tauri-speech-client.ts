import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/**
 * Raw DTOs served by the Rust speech commands (`src-tauri/src/speech_commands.rs`).
 * The Electron-shaped `window.api.speech` bridge lives in `tauri-speech-shim.ts`.
 */
export type SpeechCatalogFileEntry = {
  name: string
  sizeBytes: number
  sha256: string
}

export type SpeechCatalogEntry = {
  id: string
  label: string
  streaming: boolean
  repo: string
  revision: string
  files: SpeechCatalogFileEntry[]
}

/** Map of model id → download state string (`idle` | `downloading` | `paused` | `ready` | `failed`). */
export type SpeechModelStates = Record<string, string>

export type SpeechDownloadProgressEvent = {
  modelId: string
  fileName: string
  downloaded: number
  total: number
}

export type SpeechModelReadyEvent = {
  modelId: string
}

export type SpeechTranscriptPayload = {
  sessionId: string
  text: string
}

export type SpeechErrorPayload = {
  sessionId: string
  error: string
}

export type SpeechInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

export type SpeechListen = <T>(
  event: string,
  handler: (event: { payload: T }) => void
) => Promise<() => void>

export type TauriSpeechClient = {
  getCatalog: () => Promise<SpeechCatalogEntry[]>
  getModelStates: () => Promise<SpeechModelStates>
  downloadModel: (modelId: string) => Promise<void>
  cancelDownload: (modelId: string) => Promise<void>
  startDictation: (sessionId: string, owner: string, modelId: string) => Promise<void>
  feedAudio: (samples: Float32Array, sampleRate: number, sessionId?: string) => Promise<void>
  stopDictation: (sessionId?: string) => Promise<void>
  onPartialTranscript: (callback: (data: SpeechTranscriptPayload) => void) => () => void
  onFinalTranscript: (callback: (data: SpeechTranscriptPayload) => void) => () => void
  onDownloadProgress: (callback: (data: SpeechDownloadProgressEvent) => void) => () => void
  onModelReady: (callback: (data: SpeechModelReadyEvent) => void) => () => void
  onError: (callback: (data: SpeechErrorPayload) => void) => () => void
}

const DEFAULT_SESSION_ID = 'desktop'

/**
 * Why: Tauri `listen` resolves asynchronously while the Electron contract
 * returns a synchronous unlisten. Gate delivery on an unsubscribe flag so
 * events are dropped from the instant the caller unsubscribes, and detach the
 * underlying listener the moment the subscription settles.
 */
function subscribe<T>(
  listen: SpeechListen,
  event: string,
  callback: (data: T) => void
): () => void {
  let unsubscribed = false
  const listening = listen<T>(event, (tauriEvent) => {
    if (!unsubscribed) {
      callback(tauriEvent.payload)
    }
  })
  void listening.then(
    (unlisten) => {
      if (unsubscribed) {
        unlisten()
      }
    },
    () => undefined
  )
  return () => {
    unsubscribed = true
  }
}

/**
 * Tauri invoke-backed speech capability. Command names and event channels are
 * pinned to `src-tauri/src/speech_commands.rs` (`rename_as` + `app.emit`).
 */
export function createTauriSpeechClient(
  invoke: SpeechInvoke,
  listen: SpeechListen
): TauriSpeechClient {
  return {
    async getCatalog() {
      return (await invoke('ui_speechGetCatalog')) as SpeechCatalogEntry[]
    },
    async getModelStates() {
      return (await invoke('ui_speechGetModelStates')) as SpeechModelStates
    },
    async downloadModel(modelId) {
      await invoke('ui_speechDownloadModel', { modelId })
    },
    async cancelDownload(modelId) {
      await invoke('ui_speechCancelDownload', { modelId })
    },
    async startDictation(sessionId, owner, modelId) {
      await invoke('ui_speechStartDictation', { sessionId, owner, modelId })
    },
    async feedAudio(samples, sampleRate, sessionId = DEFAULT_SESSION_ID) {
      // Why: v1 ships PCM as a plain number[] payload (JSON-safe over invoke).
      // Upgrade path: a `tauri::ipc::Channel` or a raw ArrayBuffer invoke body
      // with little-endian f32 decoding in `ui_speech_feed_audio` once
      // per-sample JS boxing shows up in profiling.
      await invoke('ui_speechFeedAudio', {
        sessionId,
        samples: Array.from(samples),
        sampleRate
      })
    },
    async stopDictation(sessionId = DEFAULT_SESSION_ID) {
      await invoke('ui_speechStopDictation', { sessionId })
    },
    onPartialTranscript: (callback) => subscribe(listen, 'speech://partial', callback),
    onFinalTranscript: (callback) => subscribe(listen, 'speech://final', callback),
    onDownloadProgress: (callback) => subscribe(listen, 'speech://downloadProgress', callback),
    onModelReady: (callback) => subscribe(listen, 'speech://modelReady', callback),
    onError: (callback) => subscribe(listen, 'speech://error', callback)
  }
}

export const tauriSpeechClient = createTauriSpeechClient(
  (command, args) => invoke(command, args),
  listen
)
