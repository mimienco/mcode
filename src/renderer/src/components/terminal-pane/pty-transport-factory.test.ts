import { beforeEach, describe, expect, it, vi } from 'vitest'
import { isTauri } from '@tauri-apps/api/core'
import { createIpcPtyTransport } from './pty-transport'
import { createTauriPtyTransport } from './pty-transport-tauri'
import { createLocalPtyTransport } from './pty-transport-factory'

vi.mock('@tauri-apps/api/core', () => ({ isTauri: vi.fn(), invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))
vi.mock('./pty-transport', () => ({
  createIpcPtyTransport: vi.fn(),
  // The real createTauriPtyTransport composes this; provide a minimal processor
  // so the real transport body runs under the module spy below.
  createPtyOutputProcessor: () => ({
    processData: () => {},
    clearAccumulatedState: () => {},
    pausePendingSideEffects: () => {},
    flushPendingSideEffects: () => {},
    disposePendingSideEffectGauge: () => {}
  })
}))
vi.mock(import('./pty-transport-tauri'), { spy: true })

describe('createLocalPtyTransport', () => {
  const electronTransportStub = { sendInput: () => true }

  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(isTauri).mockReturnValue(false)
    vi.mocked(createIpcPtyTransport).mockReturnValue(
      electronTransportStub as unknown as ReturnType<typeof createIpcPtyTransport>
    )
  })

  it('returns the Electron IPC transport object when isTauri is false', () => {
    const opts = { cwd: 'D:/repo' }
    const transport = createLocalPtyTransport(opts)

    expect(createIpcPtyTransport).toHaveBeenCalledOnce()
    expect(createIpcPtyTransport).toHaveBeenCalledWith(opts)
    expect(transport).toBe(electronTransportStub)
    // Identity means behavior comes from the Electron transport verbatim.
    expect(transport.sendInput('dir\r\n')).toBe(true)
    expect(createTauriPtyTransport).not.toHaveBeenCalled()
  })

  it('returns the Tauri transport when isTauri is true, with an exactly (a)-row optional surface', () => {
    vi.mocked(isTauri).mockReturnValue(true)

    const transport = createLocalPtyTransport({ cwd: 'D:/repo' })

    expect(createIpcPtyTransport).not.toHaveBeenCalled()
    expect(createTauriPtyTransport).toHaveBeenCalledOnce()
    expect(createTauriPtyTransport).toHaveBeenCalledWith({ cwd: 'D:/repo' })

    // Flag-isolation drift guard (spec §3.5): absent capability-negotiated
    // optionals stay absent, and the plain handle id is unset before connect.
    expect(transport.sendInputAccepted).toBeUndefined()
    expect(transport.claimViewport).toBeUndefined()
    expect(transport.serializeBuffer).toBeUndefined()
    expect(transport.getPtyId()).toBeNull()
    // The (a)-row members exist.
    expect(typeof transport.connect).toBe('function')
    expect(typeof transport.sendInput).toBe('function')
    expect(typeof transport.sendInputImmediate).toBe('function')
    expect(typeof transport.resize).toBe('function')
    expect(typeof transport.disconnect).toBe('function')
    expect(typeof transport.attach).toBe('function')
    expect(typeof transport.isConnected).toBe('function')
    expect(typeof transport.getLocalSessionMetadata).toBe('function')
  })

  it('selects exactly once per construction', () => {
    createLocalPtyTransport({})
    expect(isTauri).toHaveBeenCalledTimes(1)

    createLocalPtyTransport({})
    expect(isTauri).toHaveBeenCalledTimes(2)
  })
})
