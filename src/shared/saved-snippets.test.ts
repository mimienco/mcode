import { describe, expect, it } from 'vitest'
import { normalizeSavedSnippets } from './saved-snippets'

describe('normalizeSavedSnippets', () => {
  it('keeps generic name and text cards without terminal execution fields', () => {
    expect(
      normalizeSavedSnippets([
        {
          id: 'git-review',
          name: 'Review Git changes',
          text: 'Review the current Git diff and explain risky changes.',
          action: 'terminal-command',
          appendEnter: true
        }
      ])
    ).toEqual([
      {
        id: 'git-review',
        name: 'Review Git changes',
        text: 'Review the current Git diff and explain risky changes.'
      }
    ])
  })

  it('drops blank, malformed, and duplicate cards', () => {
    expect(
      normalizeSavedSnippets([
        { id: 'one', name: 'One', text: 'First' },
        { id: 'one', name: 'Duplicate', text: 'Second' },
        { id: 'blank', name: 'Blank', text: '   ' },
        null
      ])
    ).toEqual([{ id: 'one', name: 'One', text: 'First' }])
  })
})
