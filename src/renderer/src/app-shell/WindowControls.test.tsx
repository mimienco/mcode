// @vitest-environment happy-dom

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import type { ComponentProps } from 'react'
import { WindowControls } from './WindowControls'

afterEach(cleanup)

type WindowControlsCapability = NonNullable<ComponentProps<typeof WindowControls>['desktopWindow']>

function createDesktopWindow(
  overrides: Partial<WindowControlsCapability> = {}
): WindowControlsCapability {
  return {
    minimize: () => Promise.resolve(),
    toggleMaximize: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onMaximizedChanged: () => Promise.resolve(() => undefined),
    requestClose: () => Promise.resolve(),
    ...overrides
  }
}

it('routes the minimize button through the desktop window client', () => {
  const minimize = vi.fn(() => Promise.resolve())
  const desktopWindow = createDesktopWindow({ minimize })

  render(<WindowControls desktopWindow={desktopWindow} />)
  fireEvent.click(screen.getByRole('button', { name: 'Minimize' }))

  expect(minimize).toHaveBeenCalledOnce()
})

it('seeds the maximize button from the native window state', async () => {
  const desktopWindow = createDesktopWindow({ isMaximized: () => Promise.resolve(true) })

  render(<WindowControls desktopWindow={desktopWindow} />)

  expect(await screen.findByRole('button', { name: 'Restore' })).toBeTruthy()
})

it('tracks native maximize changes and releases the subscription on unmount', async () => {
  let publishMaximized: ((maximized: boolean) => void) | undefined
  const unsubscribe = vi.fn()
  const desktopWindow = createDesktopWindow({
    async onMaximizedChanged(listener) {
      publishMaximized = listener
      return unsubscribe
    }
  })
  const view = render(<WindowControls desktopWindow={desktopWindow} />)

  await waitFor(() => expect(publishMaximized).toBeTypeOf('function'))
  act(() => publishMaximized?.(true))
  expect(await screen.findByRole('button', { name: 'Restore' })).toBeTruthy()

  view.unmount()
  expect(unsubscribe).toHaveBeenCalledOnce()
})

it('routes the maximize button through the desktop window client', () => {
  const toggleMaximize = vi.fn(() => Promise.resolve())
  const desktopWindow = createDesktopWindow({ toggleMaximize })

  render(<WindowControls desktopWindow={desktopWindow} />)
  fireEvent.click(screen.getByRole('button', { name: 'Maximize' }))

  expect(toggleMaximize).toHaveBeenCalledOnce()
})

it('routes the close button through the desktop window client', () => {
  const requestClose = vi.fn(() => Promise.resolve())
  const desktopWindow = createDesktopWindow({ requestClose })

  render(<WindowControls desktopWindow={desktopWindow} />)
  fireEvent.click(screen.getByRole('button', { name: 'Close' }))

  expect(requestClose).toHaveBeenCalledOnce()
})
