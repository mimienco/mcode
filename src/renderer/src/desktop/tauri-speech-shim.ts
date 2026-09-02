import type {
  SpeechErrorEvent,
  SpeechLifecycleEvent,
  SpeechModelManifest,
  SpeechModelState,
  SpeechTranscriptEvent
} from '../../../shared/speech-types'
import {
  tauriSpeechClient,
  type SpeechCatalogEntry,
  type SpeechDownloadProgressEvent
} from './tauri-speech-client'

/**
 * Ported verbatim from the Electron catalog (`src/main/speech/model-catalog.ts`):
 * the Tauri `ui_speechGetCatalog` DTO carries only downloadable local models
 * and omits the presentation fields the renderer consumes, so the shim merges
 * this metadata in and appends the cloud OpenAI entries.
 */
const CATALOG_METADATA: Record<
  string,
  { description: string; language: string; recommended?: boolean }
> = {
  'parakeet-tdt-0.6b-v3-int8': {
    description:
      'Highest accuracy for 25 European languages. Punctuation, capitalization, and word-level timestamps.',
    language: 'multilingual',
    recommended: true
  },
  'parakeet-tdt-0.6b-v2-int8': {
    description:
      'English only. Faster than v3 with similar accuracy. Punctuation and capitalization.',
    language: 'en'
  },
  'zipformer-bilingual-zh-en': {
    description: 'Chinese + English with code-switching. Low-latency real-time streaming.',
    language: 'zh-en'
  },
  'paraformer-bilingual-zh-en': {
    description:
      'Chinese (Mandarin + dialects) + English. Strong on accented and regional Chinese.',
    language: 'zh-en'
  },
  'zipformer-streaming-en-20m': {
    description: 'English only. Lightweight 20M-param model, good balance of speed and size.',
    language: 'en'
  },
  'zipformer-streaming-zh-14m': {
    description: 'Chinese only. Ultra-lightweight 14M-param model, ideal for low-resource devices.',
    language: 'zh'
  },
  'zipformer-streaming-korean': {
    description: 'Korean only. Low-latency real-time streaming.',
    language: 'ko'
  },
  'parakeet-tdt-ctc-0.6b-ja-int8': {
    description: 'Japanese only. Trained on 35k+ hours of natural speech. Punctuation included.',
    language: 'ja'
  },
  'whisper-tiny': {
    description: '90+ languages. Lower accuracy than Parakeet but broadest language coverage.',
    language: 'multilingual'
  },
  'sense-voice-zh-en-ja-ko-yue': {
    description:
      'Chinese, English, Japanese, Korean, and Cantonese with automatic language detection.',
    language: 'multilingual'
  }
}

const OPENAI_MODELS: SpeechModelManifest[] = [
  {
    id: 'openai-gpt-4o-mini-transcribe',
    label: 'GPT-4o mini Transcribe',
    description:
      'Cloud transcription with strong accuracy and low cost. Requires an OpenAI API key.',
    type: 'openai',
    provider: 'openai',
    language: 'multilingual',
    sampleRate: 16000,
    streaming: false
  },
  {
    id: 'openai-gpt-4o-transcribe',
    label: 'GPT-4o Transcribe',
    description: 'Cloud transcription with higher accuracy. Requires an OpenAI API key.',
    type: 'openai',
    provider: 'openai',
    language: 'multilingual',
    sampleRate: 16000,
    streaming: false
  }
]
/**
 * Why: the Tauri catalog entry has no `type`; the renderer only needs it to
 * tell local recognizers apart in the settings UI, which the sherpa repo name
 * pins unambiguously. Unknown future ids degrade to `whisper`.
 */
function localModelType(entry: SpeechCatalogEntry): SpeechModelManifest['type'] {
  const repo = entry.repo
  if (repo.includes('whisper')) {
    return 'whisper'
  }
  if (repo.includes('sense-voice')) {
    return 'senseVoice'
  }
  if (repo.includes('paraformer')) {
    return 'paraformer'
  }
  if (repo.includes('nemo')) {
    return 'nemo-ctc'
  }
  return 'transducer'
}

function toManifest(entry: SpeechCatalogEntry): SpeechModelManifest {
  const metadata = CATALOG_METADATA[entry.id] ?? { description: '', language: 'multilingual' }
  return {
    id: entry.id,
    label: entry.label,
    description: metadata.description,
    type: localModelType(entry),
    provider: 'local',
    language: metadata.language,
    sizeBytes: entry.files.reduce((total, file) => total + file.sizeBytes, 0),
    files: entry.files.map((file) => file.name),
    sampleRate: 16000,
    streaming: entry.streaming,
    ...(metadata.recommended ? { recommended: true } : {})
  }
}

const STATE_BY_STATUS_STRING: Record<string, SpeechModelState['status']> = {
  idle: 'not-downloaded',
  paused: 'not-downloaded',
  downloading: 'downloading',
  ready: 'ready',
  failed: 'error'
}

function toModelState(id: string, statusString: string): SpeechModelState {
  return {
    id,
    status: STATE_BY_STATUS_STRING[statusString] ?? 'not-downloaded'
  }
}

/**
 * Synthesizes the Electron `onStopped` callback: the Tauri backend emits no
 * stopped event, but `ui_speechStopDictation` resolves only after the session
 * is torn down, so resolving the stop marks the session as stopped.
 */
function makeStopMarksStopped(client: typeof tauriSpeechClient) {
  return (callback: (data: SpeechLifecycleEvent) => void): (() => void) => {
    const original = client.stopDictation.bind(client)
    client.stopDictation = async (sessionId) => {
      await original(sessionId)
      callback({ sessionId: sessionId ?? 'desktop' })
    }
    return () => {
      client.stopDictation = original
    }
  }
}

export type TauriSpeechBridge = {
  getCatalog: () => Promise<SpeechModelManifest[]>
  getModelStates: () => Promise<SpeechModelState[]>
  getOpenAiApiKeyStatus: () => Promise<{ configured: boolean }>
  saveOpenAiApiKey: (apiKey: string) => Promise<{ configured: boolean }>
  clearOpenAiApiKey: () => Promise<{ configured: boolean }>
  downloadModel: (modelId: string) => Promise<void>
  cancelDownload: (modelId: string) => Promise<void>
  deleteModel: (modelId: string) => Promise<void>
  startDictation: (
    modelId: string,
    hotwords: string[] | undefined,
    sessionId: string
  ) => Promise<void>
  feedAudio: (samples: Float32Array, sampleRate: number, sessionId?: string) => Promise<void>
  stopDictation: (sessionId?: string) => Promise<void>
  onPartialTranscript: (callback: (data: SpeechTranscriptEvent) => void) => () => void
  onFinalTranscript: (callback: (data: SpeechTranscriptEvent) => void) => () => void
  onDownloadProgress: (
    callback: (data: { modelId: string; progress: number }) => void
  ) => () => void
  onReady: (callback: (data: SpeechLifecycleEvent) => void) => () => void
  onStopped: (callback: (data: SpeechLifecycleEvent) => void) => () => void
  onError: (callback: (data: SpeechErrorEvent) => void) => () => void
}

const OWNER = 'desktop'

/**
 * Bridges the Electron `window.api.speech` shape onto the Tauri speech
 * commands (`src-tauri/src/speech_commands.rs`).
 */
export function createTauriSpeechBridge(client: typeof tauriSpeechClient): TauriSpeechBridge {
  return {
    async getCatalog() {
      const entries = await client.getCatalog()
      const local = entries.map(toManifest)
      // Why: match the Electron catalog's renderer-relevant ordering — cloud
      // models sit after the local ones in `src/main/speech/model-catalog.ts`.
      return [...local, ...OPENAI_MODELS]
    },
    async getModelStates() {
      const states = await client.getModelStates()
      return Object.entries(states).map(([id, statusString]) => toModelState(id, statusString))
    },
    // Why: no OpenAI key management exists under the Tauri runtime yet, so the
    // cloud-model path reports "configured" and no-ops to keep the Voice pane
    // renderable instead of throwing on every open.
    async getOpenAiApiKeyStatus() {
      return { configured: true }
    },
    async saveOpenAiApiKey() {
      return { configured: true }
    },
    async clearOpenAiApiKey() {
      return { configured: false }
    },
    downloadModel: (modelId) => client.downloadModel(modelId),
    cancelDownload: (modelId) => client.cancelDownload(modelId),
    // Why: Tauri v1 has no deleteModel command and scoped fs access prevents
    // renderer-side model-dir removal; report failure so the Voice pane shows
    // its delete-error toast instead of silently claiming success.
    async deleteModel() {
      throw new Error('deleteModel is not supported under the Tauri runtime yet')
    },
    async startDictation(modelId, _hotwords, sessionId) {
      // Why: hotword boosting is not wired through the Rust session service
      // yet; the Electron path writes a hotwords file the Tauri backend does
      // not consume.
      await client.startDictation(sessionId, OWNER, modelId)
    },
    feedAudio: (samples, sampleRate, sessionId) => client.feedAudio(samples, sampleRate, sessionId),
    stopDictation: (sessionId) => client.stopDictation(sessionId),
    onPartialTranscript: (callback) =>
      client.onPartialTranscript((data: SpeechTranscriptEvent) => callback(data)),
    onFinalTranscript: (callback) =>
      client.onFinalTranscript((data: SpeechTranscriptEvent) => callback(data)),
    onDownloadProgress: (callback) =>
      client.onDownloadProgress((data: SpeechDownloadProgressEvent) => {
        const { total } = data
        callback({
          modelId: data.modelId,
          progress: total > 0 ? data.downloaded / total : 0
        })
      }),
    // Why: the Rust backend emits no ready event (recognition starts lazily on
    // the first audio feed), so no ready callback can ever fire.
    onReady: () => () => undefined,
    onStopped: makeStopMarksStopped(client),
    onError: (callback) =>
      client.onError((data: SpeechErrorEvent) => {
        callback({ error: data.error, sessionId: data.sessionId })
      })
  }
}

export const tauriSpeechBridge = createTauriSpeechBridge(tauriSpeechClient)
