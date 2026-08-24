// @vitest-environment happy-dom
import '@testing-library/jest-dom/vitest'

import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

// Why: radix popover needs layout/pointer infrastructure happy-dom lacks; a tiny
// open-state shim keeps the trigger/content contract testable.
vi.mock('@/components/ui/popover', async () => {
  // Why: vi.mock factories are hoisted above static imports, so React must be
  // loaded via await import() here.
  const { createContext, useContext, useState, useCallback, useMemo } = await import('react')
  const PopoverOpenContext = createContext<{
    open: boolean
    setOpen: (open: boolean) => void
  }>({ open: false, setOpen: () => {} })

  return {
    Popover: ({
      children,
      open: controlledOpen,
      onOpenChange
    }: {
      children: ReactNode
      open?: boolean
      onOpenChange?: (open: boolean) => void
    }) => {
      const [internalOpen, setInternalOpen] = useState(false)
      const open = controlledOpen ?? internalOpen
      // Why: useMemo satisfies jsx-no-constructed-context-values in the shim.
      const setOpen = useCallback(
        (next: boolean) => {
          setInternalOpen(next)
          onOpenChange?.(next)
        },
        [onOpenChange]
      )
      const value = useMemo(() => ({ open, setOpen }), [open, setOpen])
      return <PopoverOpenContext.Provider value={value}>{children}</PopoverOpenContext.Provider>
    },
    PopoverTrigger: ({ children }: { children: ReactNode }) => {
      const { open, setOpen } = useContext(PopoverOpenContext)
      // Why: asChild renders the consumer's button; toggling via a wrapper keeps
      // the button itself untouched for aria/title assertions.
      return <div onClick={() => setOpen(!open)}>{children}</div>
    },
    PopoverContent: ({ children }: { children: ReactNode }) => {
      const { open } = useContext(PopoverOpenContext)
      if (!open) {
        return null
      }
      return <div data-testid="popover-content">{children}</div>
    },
    PopoverArrow: () => null
  }
})

vi.mock('@/components/editor/useLocalImageSrc', () => ({
  loadLocalImageAbsolutePath: vi.fn()
}))

import { loadLocalImageAbsolutePath } from '@/components/editor/useLocalImageSrc'
import { NativeChatImagePreviewChip } from './NativeChatImagePreviewChip'

const loadLocalImageAbsolutePathMock = vi.mocked(loadLocalImageAbsolutePath)

function defer<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

beforeEach(() => {
  loadLocalImageAbsolutePathMock.mockReset()
})

describe('NativeChatImagePreviewChip', () => {
  it('exposes an accessible trigger titled with the path and labeled with the given label', () => {
    render(<NativeChatImagePreviewChip path="/home/me/shots/pic.png" label="pic.png" />)

    const trigger = screen.getByRole('button', { name: 'Preview image' })
    expect(trigger).toHaveAttribute('title', '/home/me/shots/pic.png')
    expect(trigger).toHaveAttribute('type', 'button')
    expect(screen.getByText('pic.png')).toBeInTheDocument()
  })

  it('does not resolve the preview until the popover opens', async () => {
    const deferred = defer<string | null>()
    loadLocalImageAbsolutePathMock.mockReturnValue(deferred.promise)
    const { container } = render(<NativeChatImagePreviewChip path="/home/me/pic.png" label="pic.png" />)

    expect(loadLocalImageAbsolutePathMock).not.toHaveBeenCalled()
    expect(container.querySelector('.animate-pulse')).toBeNull()
    expect(screen.queryByTestId('popover-content')).toBeNull()

    deferred.resolve(null)
  })

  it('shows a pulse while loading, then the loaded image', async () => {
    const user = userEvent.setup()
    const deferred = defer<string | null>()
    loadLocalImageAbsolutePathMock.mockReturnValue(deferred.promise)
    render(<NativeChatImagePreviewChip path={WINDOWS_PATH} label="pic.png" />)

    await user.click(screen.getByRole('button', { name: 'Preview image' }))

    expect(loadLocalImageAbsolutePathMock).toHaveBeenCalledTimes(1)
    expect(loadLocalImageAbsolutePathMock).toHaveBeenCalledWith('C:\\shots\\pic.png')
    expect(document.querySelector('.animate-pulse')).not.toBeNull()

    deferred.resolve('blob:preview-url')

    const image = await screen.findByAltText('pic.png')
    expect(image).toHaveAttribute('src', 'blob:preview-url')
    expect(image).toHaveClass('object-contain')

    await waitFor(() => {
      expect(document.querySelector('.animate-pulse')).toBeNull()
    })
    expect(loadLocalImageAbsolutePathMock).toHaveBeenCalledTimes(1)
  })

  it('falls back to the unavailable message when the path cannot be loaded', async () => {
    const user = userEvent.setup()
    loadLocalImageAbsolutePathMock.mockResolvedValue(null)
    render(<NativeChatImagePreviewChip path="/gone.png" label="gone.png" />)

    await user.click(screen.getByRole('button', { name: 'Preview image' }))

    expect(await screen.findByText('Preview unavailable')).not.toBeNull()
    expect(screen.queryByAltText('gone.png')).toBeNull()
  })

  it('renders a direct src as-is without touching the local-path loader', async () => {
    const user = userEvent.setup()
    render(
      <NativeChatImagePreviewChip src="https://example.com/generated.png" label="generated.png" />
    )

    const trigger = screen.getByRole('button', { name: 'Preview image' })
    expect(trigger).toHaveAttribute('title', 'https://example.com/generated.png')

    await user.click(trigger)

    const image = await screen.findByAltText('generated.png')
    expect(image).toHaveAttribute('src', 'https://example.com/generated.png')
    expect(document.querySelector('.animate-pulse')).toBeNull()
    expect(loadLocalImageAbsolutePathMock).not.toHaveBeenCalled()
  })

  it('opens the full lightbox when the popover image is clicked', async () => {
    const user = userEvent.setup()
    loadLocalImageAbsolutePathMock.mockResolvedValue('blob:preview-url')
    render(<NativeChatImagePreviewChip path="/home/me/pic.png" label="pic.png" />)

    await user.click(screen.getByRole('button', { name: 'Preview image' }))
    await user.click(await screen.findByRole('button', { name: 'Expand image' }))

    const lightboxImage = await screen.findByAltText('pic.png')
    expect(lightboxImage).toHaveAttribute('src', 'blob:preview-url')
    expect(within(screen.getByRole('dialog')).getAllByText('pic.png').length).toBeGreaterThan(0)
  })

  it('loads a thumbnail once visible when showThumbnail is set', async () => {
    class ImmediateObserver {
      callback: IntersectionObserverCallback
      constructor(callback: IntersectionObserverCallback) {
        this.callback = callback
      }
      observe(): void {
        this.callback(
          [{ isIntersecting: true } as IntersectionObserverEntry],
          this as unknown as IntersectionObserver
        )
      }
      disconnect(): void {}
      unobserve(): void {}
      takeRecords(): IntersectionObserverEntry[] {
        return []
      }
      root = null
      rootMargin = ''
      thresholds = []
    }
    vi.stubGlobal('IntersectionObserver', ImmediateObserver)
    loadLocalImageAbsolutePathMock.mockResolvedValue('blob:thumb-url')
    render(<NativeChatImagePreviewChip path="/home/me/pic.png" label="pic.png" showThumbnail />)

    const thumb = await screen.findByAltText('')
    expect(thumb).toHaveAttribute('src', 'blob:thumb-url')
    expect(loadLocalImageAbsolutePathMock).toHaveBeenCalledTimes(1)
    vi.unstubAllGlobals()
  })
})

// Why: JSX attribute strings do not process escapes, so the backslash path
// must arrive as a JS expression value.
const WINDOWS_PATH = 'C:\\shots\\pic.png'
