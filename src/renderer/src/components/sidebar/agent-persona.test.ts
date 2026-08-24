import { describe, expect, it } from 'vitest'

import {
  findWorktreeById,
  getWorktreeAgentPersona,
  nextAgentPersonaColor
} from './agent-persona'
import type { Worktree } from '../../../../shared/worktree/types'

const worktreeId = `repo-1::wt-a`

function makeWorktree(overrides?: Partial<Worktree>): Worktree {
  return {
    id: worktreeId,
    repoId: 'repo-1',
    displayName: 'wt-a',
    comment: '',
    linkedIssue: null,
    linkedPR: null,
    linkedLinearIssue: null,
    linkedTaskSourceContext: null,
    isArchived: false,
    isUnread: false,
    isPinned: false,
    sortOrder: 0,
    lastActivityAt: 0,
    ...overrides
  } as Worktree
}

describe('agent persona helpers', () => {
  const byRepo = { 'repo-1': [makeWorktree({ personas: { codex: { name: 'Marshall', color: '#3f9142' } } })] }

  it('resolves the persona for the matching agentType', () => {
    expect(getWorktreeAgentPersona(byRepo, worktreeId, 'codex')).toEqual({
      name: 'Marshall',
      color: '#3f9142'
    })
  })

  it('returns undefined for unknown agents, worktrees and bare stores', () => {
    expect(getWorktreeAgentPersona(byRepo, worktreeId, 'claude')).toBeUndefined()
    expect(getWorktreeAgentPersona(byRepo, 'other::wt', 'codex')).toBeUndefined()
    expect(getWorktreeAgentPersona(undefined, worktreeId, 'codex')).toBeUndefined()
  })

  it('finds worktrees across separators and tolerates missing ids', () => {
    expect(findWorktreeById(byRepo, worktreeId)?.id).toBe(worktreeId)
    expect(findWorktreeById(byRepo, undefined)).toBeUndefined()
    expect(findWorktreeById({}, worktreeId)).toBeUndefined()
  })

  it('cycles colors forward and wraps around', () => {
    expect(nextAgentPersonaColor(undefined)).toBeDefined()
    const first = nextAgentPersonaColor(undefined)
    let current = first
    for (let index = 0; index < 8; index += 1) {
      current = nextAgentPersonaColor(current)
    }
    expect(current).toBe(first)
  })
})
