/**
 * @vitest-environment happy-dom
 */
import { act, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { NativeChatViewSwitcher } from './NativeChatViewSwitcher'
import { useAppStore } from '@/store'
import { folderWorkspaceKey } from '../../../../shared/workspace-scope'

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children?: ReactNode }) => children,
  TooltipTrigger: ({ children }: { children?: ReactNode }) => children,
  TooltipContent: ({ children }: { children?: ReactNode }) => <span>{children}</span>
}))

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

const mounted: { container: HTMLDivElement; root: Root }[] = []

function renderSwitcher(isChatViewMode: boolean, onToggleNativeChat = vi.fn()) {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => {
    root.render(
      <NativeChatViewSwitcher
        isChatViewMode={isChatViewMode}
        onToggleNativeChat={onToggleNativeChat}
        contextLabel="Codex"
        contextDetail="Fix titlebar polish"
        instanceId="session-one"
        chatPanelId="session-one-chat-panel"
        terminalPanelId="session-one-terminal-panel"
      />
    )
  })
  mounted.push({ container, root })
  return { container, onToggleNativeChat }
}

afterEach(() => {
  for (const { container, root } of mounted.splice(0)) {
    act(() => root.unmount())
    container.remove()
  }
  useAppStore.setState({
    activeWorkspaceKey: null,
    activeWorktreeId: null,
    folderWorkspaces: []
  })
})

describe('NativeChatViewSwitcher', () => {
  it('presents Chat and Terminal as a labeled top-level tab pair', () => {
    const { container } = renderSwitcher(false)
    const tabList = container.querySelector('[role="tablist"][aria-label="Session view"]')
    const tabs = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="tab"]'))

    expect(tabList).not.toBeNull()
    expect(tabs.map((tab) => tab.textContent?.trim())).toEqual(['Chat', 'Terminal'])
    expect(tabs.map((tab) => tab.getAttribute('aria-selected'))).toEqual(['false', 'true'])
    expect(tabs.map((tab) => tab.getAttribute('aria-controls'))).toEqual([
      'session-one-chat-panel',
      'session-one-terminal-panel'
    ])
    expect(container.textContent).toContain('Codex')
    expect(container.textContent).toContain('Fix titlebar polish')
  })

  it('labels a quick chat as having no project', () => {
    const workspaceId = 'quick-chat-1'
    useAppStore.setState({
      activeWorkspaceKey: folderWorkspaceKey(workspaceId),
      activeWorktreeId: folderWorkspaceKey(workspaceId),
      folderWorkspaces: [
        {
          id: workspaceId,
          projectGroupId: 'quick-chats',
          name: 'New chat',
          folderPath: '/app/chats/quick-chat-1',
          kind: 'quick-chat',
          linkedTask: null,
          comment: '',
          isArchived: false,
          isUnread: false,
          isPinned: false,
          sortOrder: 1,
          lastActivityAt: 0,
          createdAt: 1,
          updatedAt: 1
        }
      ]
    })

    expect(renderSwitcher(true).container.textContent).toContain('No project')
  })

  it('only toggles when the inactive view is selected', () => {
    const { container, onToggleNativeChat } = renderSwitcher(false)
    const [chatTab, terminalTab] = Array.from(
      container.querySelectorAll<HTMLButtonElement>('[role="tab"]')
    )

    act(() => terminalTab.click())
    expect(onToggleNativeChat).not.toHaveBeenCalled()

    act(() => chatTab.click())
    expect(onToggleNativeChat).toHaveBeenCalledTimes(1)
  })

  it('switches views with the tab-list arrow keys', () => {
    const { container, onToggleNativeChat } = renderSwitcher(true)
    const chatTab = container.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]')
    const terminalTab = container.querySelector<HTMLButtonElement>(
      '[role="tab"][aria-selected="false"]'
    )

    chatTab?.focus()

    act(() =>
      chatTab?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    )

    expect(onToggleNativeChat).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(terminalTab)
  })
})
