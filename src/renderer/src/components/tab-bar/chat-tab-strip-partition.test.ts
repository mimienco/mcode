// @vitest-environment happy-dom

import { renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Tab } from '../../../../shared/tab-types'
import type { TerminalTab } from '../../../../shared/terminal-tab-types'
import type { TabBarProps } from './tab-bar-props'
import { createUnifiedTabLookup } from './tab-bar-item-model'
import { useTabBarItemProjection } from './use-tab-bar-item-projection'

function makeTerminal(id: string): TerminalTab {
  return {
    id,
    ptyId: null,
    worktreeId: 'worktree-1',
    title: id,
    customTitle: null,
    color: null,
    sortOrder: 0,
    createdAt: 0
  }
}

function makeUnifiedTerminal(id: string, viewMode?: 'terminal' | 'chat'): Tab {
  return {
    id,
    entityId: id,
    groupId: 'group-1',
    worktreeId: 'worktree-1',
    contentType: 'terminal',
    label: id,
    customLabel: null,
    color: null,
    sortOrder: 0,
    createdAt: 0,
    viewMode
  }
}

describe('tab-strip view switching', () => {
  const tabs = [makeTerminal('chat-1'), makeTerminal('shell-1'), makeTerminal('chat-2')]
  const unifiedTabs = [
    makeUnifiedTerminal('chat-1', 'chat'),
    makeUnifiedTerminal('shell-1', 'terminal'),
    makeUnifiedTerminal('chat-2', 'chat')
  ]

  function renderProjection(activeTabId: string) {
    const props: TabBarProps = {
      tabs,
      activeTabId,
      activeTabType: 'terminal',
      worktreeId: 'worktree-1',
      groupId: 'group-1',
      expandedPaneByTabId: {},
      onActivate: () => {},
      onClose: () => {},
      onCloseOthers: () => {},
      onCloseToRight: () => {},
      onCloseToLeft: () => {},
      onNewTerminalTab: () => {},
      onNewBrowserTab: () => {},
      onSetCustomTitle: () => {},
      onSetTabColor: () => {},
      onTogglePaneExpand: () => {}
    }
    return renderHook(() =>
      useTabBarItemProjection({
        props,
        resolvedGroupId: 'group-1',
        unifiedTabs,
        unifiedTabByVisibleId: createUnifiedTabLookup(unifiedTabs, 'group-1'),
        generatedTabTitlesEnabled: false,
        statusByRelativePath: new Map()
      })
    )
  }

  it('keeps the complete workspace strip while Chat is active', () => {
    const { result } = renderProjection('chat-1')

    expect(result.current.orderedItems.map((item) => item.id)).toEqual([
      'chat-1',
      'shell-1',
      'chat-2'
    ])
  })

  it('keeps the complete workspace strip while Terminal is active', () => {
    const { result } = renderProjection('shell-1')

    expect(result.current.orderedItems.map((item) => item.id)).toEqual([
      'chat-1',
      'shell-1',
      'chat-2'
    ])
  })
})
