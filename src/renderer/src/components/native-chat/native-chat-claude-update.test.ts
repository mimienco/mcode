import { describe, expect, it } from 'vitest'
import type { Tab } from '../../../../shared/tab-types'
import { resolveNativeChatUpdateTarget } from './native-chat-claude-update'

function terminalTab(overrides: Partial<Tab> = {}): Tab {
  return {
    id: 'unified-1',
    entityId: 'terminal-1',
    groupId: 'group-1',
    worktreeId: 'worktree-1',
    contentType: 'terminal',
    label: 'Claude',
    customLabel: null,
    color: null,
    sortOrder: 0,
    createdAt: 1,
    ...overrides
  }
}

describe('resolveNativeChatUpdateTarget', () => {
  it('keeps the update terminal on the conversation workspace and split group', () => {
    expect(
      resolveNativeChatUpdateTarget(
        { 'worktree-1': [terminalTab()], 'worktree-2': [] },
        'terminal-1'
      )
    ).toEqual({ worktreeId: 'worktree-1', groupId: 'group-1' })
  })

  it('returns null when the conversation tab is no longer mounted', () => {
    expect(resolveNativeChatUpdateTarget({ 'worktree-1': [terminalTab()] }, 'missing')).toBeNull()
  })
})
