// @vitest-environment happy-dom

import { describe, expect, it } from 'vitest'
import type { ManagedPane, PaneManager } from '@/lib/pane-manager/pane-manager'
import { resolveQuickCommandTerminalDropPane } from './terminal-drop-pane-resolution'

function createPane(id: number, leafId: string): ManagedPane {
  const container = document.createElement('div')
  const target = document.createElement('span')
  container.appendChild(target)
  document.body.appendChild(container)
  return { id, leafId, container } as unknown as ManagedPane
}

describe('resolveQuickCommandTerminalDropPane', () => {
  it('leaves a chat leaf for the composer while accepting its terminal sibling', () => {
    const chatPane = createPane(1, 'chat-leaf')
    const terminalPane = createPane(2, 'terminal-leaf')
    const manager = {
      getPanes: () => [chatPane, terminalPane],
      getActivePane: () => chatPane
    } as unknown as PaneManager

    expect(
      resolveQuickCommandTerminalDropPane({
        manager,
        dropTarget: chatPane.container.firstChild,
        chatLeafId: 'chat-leaf',
        chatViewEnabled: true
      })
    ).toBeNull()
    expect(
      resolveQuickCommandTerminalDropPane({
        manager,
        dropTarget: terminalPane.container.firstChild,
        chatLeafId: 'chat-leaf',
        chatViewEnabled: true
      })
    ).toBe(terminalPane)

    chatPane.container.remove()
    terminalPane.container.remove()
  })
})
