import { describe, expect, it, vi } from 'vitest'
import { scheduleShellChunkWarmQueue } from './prefetch-shell-chunks'

describe('scheduleShellChunkWarmQueue', () => {
  it('warms one chunk at a time and schedules the next only after settlement', async () => {
    const scheduled: (() => void)[] = []
    const schedule = vi.fn((callback: () => void) => {
      scheduled.push(callback)
      return vi.fn()
    })
    let resolveFirst: (() => void) | undefined
    const first = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveFirst = resolve
        })
    )
    const second = vi.fn(async () => {})

    scheduleShellChunkWarmQueue([first, second], schedule)
    expect(scheduled).toHaveLength(1)

    scheduled.shift()?.()
    expect(first).toHaveBeenCalledOnce()
    expect(second).not.toHaveBeenCalled()
    expect(scheduled).toHaveLength(0)

    resolveFirst?.()
    await vi.waitFor(() => expect(scheduled).toHaveLength(1))
    scheduled.shift()?.()
    await vi.waitFor(() => expect(second).toHaveBeenCalledOnce())
  })

  it('cancels pending warmup work', () => {
    const scheduled: (() => void)[] = []
    const cancel = vi.fn()
    const schedule = vi.fn((callback: () => void) => {
      scheduled.push(callback)
      return cancel
    })
    const importer = vi.fn(async () => {})

    const stop = scheduleShellChunkWarmQueue([importer], schedule)
    stop()
    scheduled.shift()?.()

    expect(cancel).toHaveBeenCalledOnce()
    expect(importer).not.toHaveBeenCalled()
  })
})
