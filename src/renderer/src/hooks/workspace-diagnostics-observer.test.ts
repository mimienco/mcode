import { describe, expect, it } from 'vitest'

import { findWorktreeIdForPty } from './workspace-diagnostics-observer'

describe('findWorktreeIdForPty', () => {
  it('resolves tab-level, split-pane, and remote runtime PTY ownership', () => {
    const state = {
      tabsByWorktree: {
        'wt-local': [{ id: 'tab-local', ptyId: 'pty-local' }],
        'wt-split': [{ id: 'tab-split' }],
        'wt-remote': [{ id: 'tab-remote' }]
      },
      ptyIdsByTabId: {
        'tab-local': ['pty-local'],
        'tab-split': ['pty-split'],
        'tab-remote': ['remote:env-1@@terminal-1']
      },
      terminalLayoutsByTabId: {
        'tab-split': { ptyIdsByLeafId: { leaf: 'pty-split' } }
      }
    }

    expect(findWorktreeIdForPty(state, 'pty-local')).toBe('wt-local')
    expect(findWorktreeIdForPty(state, 'pty-split')).toBe('wt-split')
    expect(findWorktreeIdForPty(state, 'remote:env-1@@terminal-1')).toBe('wt-remote')
    expect(findWorktreeIdForPty(state, 'missing')).toBeNull()
  })
})
