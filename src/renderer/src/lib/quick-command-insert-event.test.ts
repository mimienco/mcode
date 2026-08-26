// @vitest-environment happy-dom

import { describe, expect, it, vi } from 'vitest'
import {
  dispatchQuickCommandChatInsert,
  dispatchQuickCommandInsert,
  subscribeQuickCommandChatInsert,
  subscribeQuickCommandInsert
} from './quick-command-insert-event'

describe('quick command insert event', () => {
  it('routes insertion only to the addressed terminal tab', () => {
    const first = vi.fn()
    const second = vi.fn()
    const releaseFirst = subscribeQuickCommandInsert('tab-1', first)
    const releaseSecond = subscribeQuickCommandInsert('tab-2', second)

    dispatchQuickCommandInsert('tab-2', 'Review this diff', 42)

    expect(first).not.toHaveBeenCalled()
    expect(second).toHaveBeenCalledWith('Review this diff', 42)
    releaseFirst()
    releaseSecond()
  })

  it('routes chat insertion to one exact split leaf', () => {
    const first = vi.fn()
    const second = vi.fn()
    const releaseFirst = subscribeQuickCommandChatInsert('tab-1', 'leaf-1', first)
    const releaseSecond = subscribeQuickCommandChatInsert('tab-1', 'leaf-2', second)

    dispatchQuickCommandChatInsert('tab-1', 'leaf-2', 'Review this diff')

    expect(first).not.toHaveBeenCalled()
    expect(second).toHaveBeenCalledWith('Review this diff')
    releaseFirst()
    releaseSecond()
  })
})
