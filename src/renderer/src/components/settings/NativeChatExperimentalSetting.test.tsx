// @vitest-environment happy-dom

import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { getDefaultSettings } from '../../../../shared/constants'
import type { GlobalSettings } from '../../../../shared/global-settings-types'
import { i18n } from '../../i18n/i18n'
import { NativeChatExperimentalSetting } from './NativeChatExperimentalSetting'

vi.mock('../../store', () => ({
  useAppStore: (selector: (state: { settingsSearchQuery: string }) => unknown) =>
    selector({ settingsSearchQuery: '' })
}))

vi.mock('../ui/select', async () => {
  const React = await import('react')

  const SelectContext = React.createContext<{
    onValueChange?: (value: string) => void
  }>({})

  return {
    Select: ({
      value,
      onValueChange,
      children
    }: {
      value: string
      onValueChange: (value: string) => void
      children: React.ReactNode
    }) => {
      const contextValue = React.useMemo(() => ({ onValueChange }), [onValueChange])
      return (
        <SelectContext.Provider value={contextValue}>
          <div data-slot="native-chat-default-view-select" data-value={value}>
            {children}
          </div>
        </SelectContext.Provider>
      )
    },
    SelectTrigger: ({ children, ...props }: React.ComponentProps<'button'> & { size?: string }) => (
      <button type="button" data-slot="select-trigger" {...props}>
        {children}
      </button>
    ),
    SelectValue: () => null,
    SelectContent: ({ children }: { children: React.ReactNode }) => (
      <div data-slot="select-content">{children}</div>
    ),
    SelectItem: ({ value, children }: { value: string; children: React.ReactNode }) => {
      const { onValueChange } = React.useContext(SelectContext)
      return (
        <button
          type="button"
          data-slot="select-item"
          data-value={value}
          onClick={() => onValueChange?.(value)}
        >
          {children}
        </button>
      )
    }
  }
})

async function renderSetting(args: {
  updateSettings: (updates: Partial<GlobalSettings>) => void
  settings: GlobalSettings
}): Promise<{ root: Root; container: HTMLElement }> {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  await act(async () => {
    root.render(
      <NativeChatExperimentalSetting
        settings={args.settings}
        updateSettings={args.updateSettings}
      />
    )
  })
  return { root, container }
}

describe('NativeChatExperimentalSetting', () => {
  afterEach(async () => {
    await i18n.changeLanguage('en')
    document.body.innerHTML = ''
  })

  it('shows the default-view child setting only when Chat UI is enabled', async () => {
    const updateSettings = vi.fn()
    const disabledMarkupContainer = document.createElement('div')
    document.body.appendChild(disabledMarkupContainer)
    const disabledRoot = createRoot(disabledMarkupContainer)
    await act(async () => {
      disabledRoot.render(
        <NativeChatExperimentalSetting
          settings={{ ...getDefaultSettings('/tmp'), experimentalNativeChat: false }}
          updateSettings={vi.fn()}
        />
      )
    })
    expect(disabledMarkupContainer.textContent).toContain('Chat UI')
    expect(disabledMarkupContainer.textContent).not.toContain('Default view')
    disabledRoot.unmount()

    const settings = {
      ...getDefaultSettings('/tmp'),
      experimentalNativeChat: true,
      openAgentTabsInChatByDefault: false
    }
    const { root, container } = await renderSetting({ updateSettings, settings })

    expect(container.textContent).toContain('Default view')
    expect(container.textContent).toContain('Terminal chat')
    expect(container.textContent).toContain('Chat UI')
    expect(
      container
        .querySelector('[data-slot="native-chat-default-view-select"]')
        ?.getAttribute('data-value')
    ).toBe('terminal-chat')

    const nativeChatOption = Array.from(
      container.querySelectorAll<HTMLButtonElement>('[data-slot="select-item"]')
    ).find((button) => button.getAttribute('data-value') === 'native-chat')
    if (!nativeChatOption) {
      throw new Error('Chat UI default-view option was not rendered')
    }

    await act(async () => {
      nativeChatOption.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })

    expect(updateSettings).toHaveBeenCalledWith({ openAgentTabsInChatByDefault: true })

    root.unmount()

    const nativeSettings = {
      ...settings,
      openAgentTabsInChatByDefault: true
    }
    const secondRender = await renderSetting({ updateSettings, settings: nativeSettings })

    expect(
      secondRender.container
        .querySelector('[data-slot="native-chat-default-view-select"]')
        ?.getAttribute('data-value')
    ).toBe('native-chat')

    const terminalChatOption = Array.from(
      secondRender.container.querySelectorAll<HTMLButtonElement>('[data-slot="select-item"]')
    ).find((button) => button.getAttribute('data-value') === 'terminal-chat')
    if (!terminalChatOption) {
      throw new Error('Terminal chat default-view option was not rendered')
    }

    await act(async () => {
      terminalChatOption.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })

    expect(updateSettings).toHaveBeenCalledWith({ openAgentTabsInChatByDefault: false })

    secondRender.root.unmount()
  })
})
