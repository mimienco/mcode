import { describe, expect, it } from 'vitest'
import { createPtySmokeOutputTracker } from './pty-smoke-output'

describe('PTY smoke output accounting', () => {
  it('finds split echoes and credits controls, emoji, salvage and trailing output cumulatively', () => {
    const observe = createPtySmokeOutputTracker('hello')
    expect(observe({ id: 1, chunks: [{ data: '\x1b[0mhel', seq: 7 }] }))
      .toEqual({ processedChars: 7, sawEcho: false })
    expect(observe({ id: 1, chunks: [{ data: 'lo😀\r\n', seq: 13 }] }))
      .toEqual({ processedChars: 13, sawEcho: true })
    expect(observe({ id: 1, chunks: [], dropped: { data: '\x1b[6n', droppedOutput: true } }))
      .toEqual({ processedChars: 17, sawEcho: true })
    expect(observe({ id: 1, chunks: [{ data: '\x1b[?25h', seq: 99 }] }))
      .toEqual({ processedChars: 23, sawEcho: true })
    expect(observe({ id: 2, chunks: [{ data: 'x', seq: 1 }] }))
      .toEqual({ processedChars: 1, sawEcho: false })
  })
})
