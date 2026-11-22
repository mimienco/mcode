import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  createTauriSpeechClient,
  type SpeechInvoke,
  type SpeechListen,
  type TauriSpeechClient
} from './tauri-speech-client'

type InvokeCall = { command: string; args?: Record<string, unknown> }

type TestInvoke = SpeechInvoke & { calls: InvokeCall[] }

function createInvoke(responses: Record<string, unknown> = {}): TestInvoke {
  const calls: InvokeCall[] = []
  const invoke = (async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    if (command in responses) {
      return responses[command]
    }
    return undefined
  }) as TestInvoke
  invoke.calls = calls
  return invoke
}

type ListenCall = { event: string }

type TestListen = SpeechListen & {
  calls: ListenCall[]
  emit: (event: string, payload: unknown) => void
}

function createListen(): TestListen {
  const calls: ListenCall[] = []
  const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>()
  const listen = (async (event: string, handler: (event: { payload: unknown }) => void) => {
    calls.push({ event })
    const bucket = handlers.get(event) ?? []
    bucket.push(handler)
    handlers.set(event, bucket)
    return () => {
      const current = handlers.get(event) ?? []
      handlers.set(
        event,
        current.filter((candidate) => candidate !== handler)
      )
    }
  }) as TestListen
  listen.calls = calls
  listen.emit = (event, payload) => {
    for (const handler of handlers.get(event) ?? []) {
      handler({ payload })
    }
  }
  return listen
}

describe('tauri-speech-client', () => {
  let invoke: TestInvoke
  let listen: TestListen
  let client: TauriSpeechClient

  beforeEach(() => {
    invoke = createInvoke()
    listen = createListen()
    client = createTauriSpeechClient(invoke, listen)
  })

  it('routes catalog and model-state reads to the renamed Rust commands', async () => {
    const catalog = [
      {
        id: 'whisper-tiny',
        label: 'Whisper Tiny',
        streaming: false,
        repo: 'csukuangfj/sherpa-onnx-whisper-tiny',
        revision: 'abc',
        files: [{ name: 'tokens.txt', sizeBytes: 93939, sha256: 'def' }]
      }
    ]
    const states = { 'whisper-tiny': 'ready' }
    invoke = createInvoke({
      ui_speechGetCatalog: catalog,
      ui_speechGetModelStates: states
    })
    client = createTauriSpeechClient(invoke, listen)

    await expect(client.getCatalog()).resolves.toEqual(catalog)
    await expect(client.getModelStates()).resolves.toEqual(states)

    expect(invoke.calls).toEqual([
      { command: 'ui_speechGetCatalog', args: undefined },
      { command: 'ui_speechGetModelStates', args: undefined }
    ])
  })

  it('routes downloads and dictation control to the renamed Rust commands', async () => {
    await client.downloadModel('whisper-tiny')
    await client.cancelDownload('whisper-tiny')
    await client.startDictation('session-1', 'desktop', 'whisper-tiny')
    await client.stopDictation('session-1')
    await client.stopDictation()

    expect(invoke.calls).toEqual([
      { command: 'ui_speechDownloadModel', args: { modelId: 'whisper-tiny' } },
      { command: 'ui_speechCancelDownload', args: { modelId: 'whisper-tiny' } },
      {
        command: 'ui_speechStartDictation',
        args: { sessionId: 'session-1', owner: 'desktop', modelId: 'whisper-tiny' }
      },
      { command: 'ui_speechStopDictation', args: { sessionId: 'session-1' } },
      { command: 'ui_speechStopDictation', args: { sessionId: 'desktop' } }
    ])
  })

  it('feeds PCM samples as a plain number array with the default session id', async () => {
    const samples = new Float32Array([0.25, -0.5])
    await client.feedAudio(samples, 16000)

    expect(invoke.calls).toEqual([
      {
        command: 'ui_speechFeedAudio',
        args: { sessionId: 'desktop', samples: [0.25, -0.5], sampleRate: 16000 }
      }
    ])
  })

  it('honors an explicit session id for feedAudio', async () => {
    await client.feedAudio(new Float32Array([1]), 48000, 'session-9')

    expect(invoke.calls).toEqual([
      {
        command: 'ui_speechFeedAudio',
        args: { sessionId: 'session-9', samples: [1], sampleRate: 48000 }
      }
    ])
  })

  it('subscribes to the five Rust event channels and unwraps the tauri payload', () => {
    const onPartial = vi.fn()
    const onFinal = vi.fn()
    const onProgress = vi.fn()
    const onReady = vi.fn()
    const onError = vi.fn()

    const unlistenPartial = client.onPartialTranscript(onPartial)
    client.onFinalTranscript(onFinal)
    client.onDownloadProgress(onProgress)
    client.onModelReady(onReady)
    client.onError(onError)

    expect(listen.calls.map((call) => call.event)).toEqual([
      'speech://partial',
      'speech://final',
      'speech://downloadProgress',
      'speech://modelReady',
      'speech://error'
    ])

    listen.emit('speech://partial', { sessionId: 's1', text: 'hel' })
    listen.emit('speech://final', { sessionId: 's1', text: 'hello' })
    listen.emit('speech://downloadProgress', {
      modelId: 'whisper-tiny',
      fileName: 'tokens.txt',
      downloaded: 50,
      total: 100
    })
    listen.emit('speech://modelReady', { modelId: 'whisper-tiny' })
    listen.emit('speech://error', { sessionId: 's1', error: 'boom' })

    expect(onPartial).toHaveBeenCalledWith({ sessionId: 's1', text: 'hel' })
    expect(onFinal).toHaveBeenCalledWith({ sessionId: 's1', text: 'hello' })
    expect(onProgress).toHaveBeenCalledWith({
      modelId: 'whisper-tiny',
      fileName: 'tokens.txt',
      downloaded: 50,
      total: 100
    })
    expect(onReady).toHaveBeenCalledWith({ modelId: 'whisper-tiny' })
    expect(onError).toHaveBeenCalledWith({ sessionId: 's1', error: 'boom' })

    unlistenPartial()
    listen.emit('speech://partial', { sessionId: 's1', text: 'gone' })
    expect(onPartial).toHaveBeenCalledTimes(1)
  })

  it('unlistens a subscription resolved after the caller already unsubscribed', async () => {
    let resolveListen: ((unlisten: () => void) => void) | undefined
    const deferredListen = (async () =>
      new Promise<() => void>((resolve) => {
        resolveListen = resolve
      })) as unknown as SpeechListen

    const deferredClient = createTauriSpeechClient(invoke, deferredListen)
    const unlisten = deferredClient.onPartialTranscript(vi.fn())
    unlisten()

    const innerUnlisten = vi.fn()
    resolveListen?.(innerUnlisten)
    await vi.waitFor(() => {
      expect(innerUnlisten).toHaveBeenCalledTimes(1)
    })

    expect(innerUnlisten).toHaveBeenCalledTimes(1)
  })

  it.each([
    'onPartialTranscript',
    'onFinalTranscript',
    'onDownloadProgress',
    'onModelReady',
    'onError'
  ] as const)('detaches an armed %s subscription exactly once', async (method) => {
    const innerUnlisten = vi.fn()
    const armedListen: SpeechListen = async () => innerUnlisten
    const armedClient = createTauriSpeechClient(invoke, armedListen)
    const unlisten = armedClient[method](vi.fn())

    await Promise.resolve()
    unlisten()
    unlisten()

    expect(innerUnlisten).toHaveBeenCalledTimes(1)
  })

  it('allows cleanup when native subscription setup fails', async () => {
    const rejectedListen: SpeechListen = async () => {
      throw new Error('listener setup failed')
    }
    const rejectedClient = createTauriSpeechClient(invoke, rejectedListen)
    const unlisten = rejectedClient.onPartialTranscript(vi.fn())

    await Promise.resolve()
    expect(() => {
      unlisten()
      unlisten()
    }).not.toThrow()
  })
})
