import { describe, expect, it } from 'vitest'
import { sanitizeWorktreeName } from './worktree-name'

describe('sanitizeWorktreeName', () => {
  it('normalizes filesystem and Git-ref punctuation before showing or creating a workspace', () => {
    expect(sanitizeWorktreeName('bad<>|?*name')).toBe('bad-name')
    expect(sanitizeWorktreeName('release.lock')).toBe('release-lock')
  })

  it('keeps Unicode names and makes Windows device basenames portable', () => {
    expect(sanitizeWorktreeName('日本語 テスト')).toBe('日本語-テスト')
    expect(sanitizeWorktreeName('CON.txt')).toBe('workspace-CON.txt')
  })

  it('rejects input that has no usable name', () => {
    expect(() => sanitizeWorktreeName('<>|?*')).toThrow('Invalid worktree name')
  })
})
