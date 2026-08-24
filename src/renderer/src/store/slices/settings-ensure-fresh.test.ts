import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { createTestStore } from './store-test-helpers'

const settingsGet = vi.fn()
const settingsSet = vi.fn().mockResolvedValue(undefined)
const keybindingsGet = vi.fn()

beforeEach(() => {
  vi.clearAllMocks()
  settingsGet.mockResolvedValue({ notifications: {} })
  keybindingsGet.mockResolvedValue({ overrides: {}, path: '/k', exists: true })
  vi.stubGlobal('window', {
    api: {
      settings: { get: settingsGet, set: settingsSet },
      keybindings: { get: keybindingsGet }
    }
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

// PERF-17: Settings mount must not repeat blocking hydration that startup
// already ran. Identical reads coalesce into one IPC call, a fresh snapshot
// suppresses the refetch until the freshness window lapses, and the plain
// fetch actions stay hard refreshes for explicit user intent.
describe('settings query freshness gates', () => {
  it('coalesces ten concurrent ensureSettingsFresh calls into one settingsGet', async () => {
    const store = createTestStore()

    await Promise.all(Array.from({ length: 10 }, () => store.getState().ensureSettingsFresh()))

    expect(settingsGet).toHaveBeenCalledTimes(1)
  })

  it('skips the refetch while fresh and revalidates after the window lapses', async () => {
    vi.useFakeTimers()
    const store = createTestStore()

    await store.getState().ensureSettingsFresh()
    expect(settingsGet).toHaveBeenCalledTimes(1)

    await store.getState().ensureSettingsFresh()
    expect(settingsGet).toHaveBeenCalledTimes(1)

    vi.advanceTimersByTime(31_000)
    await store.getState().ensureSettingsFresh()
    expect(settingsGet).toHaveBeenCalledTimes(2)
  })

  it('keeps fetchSettings as a hard refresh that always reads', async () => {
    const store = createTestStore()

    await store.getState().fetchSettings()
    await store.getState().fetchSettings()

    expect(settingsGet).toHaveBeenCalledTimes(2)
  })

  it('coalesces concurrent ensureKeybindingsFresh calls into one keybindingsGet', async () => {
    const store = createTestStore()

    await Promise.all(Array.from({ length: 5 }, () => store.getState().ensureKeybindingsFresh()))

    expect(keybindingsGet).toHaveBeenCalledTimes(1)
  })
})
