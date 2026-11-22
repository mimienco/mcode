import { describe, expect, it } from 'vitest'
import {
  isExpectedWatcherProcessCancellation,
  WatcherProcessFailure
} from './parcel-watcher-process-failure'

describe('isExpectedWatcherProcessCancellation', () => {
  it('recognizes the subscription abort produced by app teardown', () => {
    expect(
      isExpectedWatcherProcessCancellation(
        new WatcherProcessFailure(
          'file watcher subscription aborted',
          'subscription',
          'subscribe_aborted'
        )
      )
    ).toBe(true)
  })

  it('keeps real watcher failures reportable', () => {
    expect(
      isExpectedWatcherProcessCancellation(
        new WatcherProcessFailure('watcher crashed', 'supervisor', 'process_unavailable')
      )
    ).toBe(false)
    expect(isExpectedWatcherProcessCancellation(new Error('unknown failure'))).toBe(false)
  })
})
