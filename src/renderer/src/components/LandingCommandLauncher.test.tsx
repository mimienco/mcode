// @vitest-environment happy-dom

import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LandingCommandLauncher, type LandingCommandLauncherProps } from './LandingCommandLauncher'

const shortcut = (keys: string[]) => ({ keys, doubleTap: false })

function createProps(
  overrides: Partial<LandingCommandLauncherProps> = {}
): LandingCommandLauncherProps {
  return {
    quickChatAvailable: true,
    creatingQuickChat: false,
    newChatLabel: 'New Chat',
    addProjectLabel: 'Add Project',
    createTargetActionLabel: 'New Workspace',
    createTargetShortcut: shortcut(['Ctrl', 'N']),
    previousWorkspaceShortcut: shortcut(['Ctrl', 'Shift', '↑']),
    nextWorkspaceShortcut: shortcut(['Ctrl', 'Shift', '↓']),
    onCreateQuickChat: vi.fn(),
    onCreateTarget: vi.fn(),
    onAddProject: vi.fn(),
    ...overrides
  }
}

const roots: Root[] = []

async function renderLauncher(props: LandingCommandLauncherProps): Promise<HTMLDivElement> {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  roots.push(root)

  await act(async () => {
    root.render(<LandingCommandLauncher {...props} />)
  })

  return container
}

function getButton(container: HTMLElement, label: string): HTMLButtonElement {
  const button = [...container.querySelectorAll('button')].find((candidate) =>
    candidate.textContent?.includes(label)
  )
  if (!button) {
    throw new Error(`Button not found: ${label}`)
  }
  return button
}

describe('LandingCommandLauncher', () => {
  beforeEach(() => {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  })

  afterEach(() => {
    roots.splice(0).forEach((root) => {
      act(() => root.unmount())
    })
    document.body.replaceChildren()
    vi.clearAllMocks()
  })

  it('promotes quick chat and keeps creation and navigation shortcuts with their controls', async () => {
    const container = await renderLauncher(createProps())

    expect(getButton(container, 'New Chat').dataset.landingCommand).toBe('primary')
    const workspaceButton = getButton(container, 'New Workspace')
    expect(workspaceButton.dataset.landingCommand).toBe('secondary')
    expect(workspaceButton.textContent).toContain('Ctrl')
    expect(workspaceButton.textContent).toContain('N')
    expect(container.textContent).toContain('Move up workspace')
    expect(container.textContent).toContain('Move down workspace')
    expect(container.querySelector('[data-landing-connectors]')).not.toBeNull()
  })

  it('preserves all three launch actions', async () => {
    const props = createProps()
    const container = await renderLauncher(props)

    await act(async () => {
      getButton(container, 'New Chat').dispatchEvent(new MouseEvent('click', { bubbles: true }))
      getButton(container, 'New Workspace').dispatchEvent(
        new MouseEvent('click', { bubbles: true })
      )
      getButton(container, 'Add Project').dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })

    expect(props.onCreateQuickChat).toHaveBeenCalledOnce()
    expect(props.onCreateTarget).toHaveBeenCalledOnce()
    expect(props.onAddProject).toHaveBeenCalledOnce()
  })

  it('promotes workspace creation when quick chat is unavailable', async () => {
    const container = await renderLauncher(createProps({ quickChatAvailable: false }))

    expect(
      [...container.querySelectorAll('button')].some((button) => button.textContent === 'New Chat')
    ).toBe(false)
    expect(getButton(container, 'New Workspace').dataset.landingCommand).toBe('primary')
    expect(getButton(container, 'Add Project').dataset.landingCommand).toBe('secondary')
  })
})
