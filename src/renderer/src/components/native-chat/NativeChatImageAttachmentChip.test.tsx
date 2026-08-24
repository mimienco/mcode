// @vitest-environment happy-dom
import '@testing-library/jest-dom/vitest'

import { cleanup, render, screen, waitFor } from '@testing-library/react'
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
import { NativeChatImageAttachmentChip } from './NativeChatImageAttachmentChip'

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

describe('NativeChatImageAttachmentChip', () => {
  it('shows the pasted-image label for clipboard paste temp paths', () => {
    render(
      <NativeChatImageAttachmentChip
        attachment={{ id: 'a1', path: 'C:\\Users\\me\\AppData\\Local\\Temp\\mcode-paste-123-abc.png' }}
        onRemove={vi.fn()}
      />
    )

    expect(screen.getByText('Pasted image')).not.toBeNull()
  })

  it('shows the basename for regular image paths', () => {
    render(
      <NativeChatImageAttachmentChip
        attachment={{ id: 'a1', path: '/home/me/shots/screenshot.png' }}
        onRemove={vi.fn()}
      />
    )

    expect(screen.getByText('screenshot.png')).not.toBeNull()
  })

  it('invokes onRemove with the attachment id from the remove button', async () => {
    const user = userEvent.setup()
    const onRemove = vi.fn()
    render(
      <NativeChatImageAttachmentChip
        attachment={{ id: 'a1', path: '/home/me/shots/screenshot.png' }}
        onRemove={onRemove}
      />
    )

    await user.click(screen.getByRole('button', { name: 'Remove attachment' }))

    expect(onRemove).toHaveBeenCalledTimes(1)
    expect(onRemove).toHaveBeenCalledWith('a1')
  })

  it('exposes an accessible preview trigger titled with the attachment path', () => {
    const path = '/home/me/shots/screenshot.png'
    render(<NativeChatImageAttachmentChip attachment={{ id: 'a1', path }} onRemove={vi.fn()} />)

    const previewButton = screen.getByRole('button', { name: 'Preview image' })
    expect(previewButton).toHaveAttribute('title', path)
    expect(previewButton).toHaveAttribute('type', 'button')
  })

  it('does not resolve the preview until the popover opens', async () => {
    const deferred = defer<string | null>()
    loadLocalImageAbsolutePathMock.mockReturnValue(deferred.promise)
    const { container } = render(
      <NativeChatImageAttachmentChip attachment={{ id: 'a1', path: '/home/me/pic.png' }} onRemove={vi.fn()} />
    )

    expect(loadLocalImageAbsolutePathMock).not.toHaveBeenCalled()
    expect(container.querySelector('.animate-pulse')).toBeNull()

    deferred.resolve(null)
  })

  it('loads once and shows the image when opened', async () => {
    const user = userEvent.setup()
    const deferred = defer<string | null>()
    loadLocalImageAbsolutePathMock.mockReturnValue(deferred.promise)
    render(
      <NativeChatImageAttachmentChip
        attachment={{ id: 'a1', path: 'C:\\shots\\pic.png' }}
        onRemove={vi.fn()}
      />
    )

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

  it('shows the pasted-image alt label for paste temp files', async () => {
    const user = userEvent.setup()
    loadLocalImageAbsolutePathMock.mockResolvedValue('blob:pasted')
    render(
      <NativeChatImageAttachmentChip
        attachment={{ id: 'a1', path: 'C:\\Temp\\mcode-paste-9-xyz.png' }}
        onRemove={vi.fn()}
      />
    )

    await user.click(screen.getByRole('button', { name: 'Preview image' }))

    expect(await screen.findByAltText('Pasted image')).toHaveAttribute('src', 'blob:pasted')
  })

  it('falls back to the unavailable message when the preview cannot be loaded', async () => {
    const user = userEvent.setup()
    loadLocalImageAbsolutePathMock.mockResolvedValue(null)
    render(<NativeChatImageAttachmentChip attachment={{ id: 'a1', path: '/gone.png' }} onRemove={vi.fn()} />)

    await user.click(screen.getByRole('button', { name: 'Preview image' }))

    expect(await screen.findByText('Preview unavailable')).not.toBeNull()
    expect(screen.queryByAltText('gone.png')).toBeNull()
  })

  it('resolves again after closing and reopening the popover', async () => {
    const user = userEvent.setup()
    loadLocalImageAbsolutePathMock.mockResolvedValue('blob:again')
    render(<NativeChatImageAttachmentChip attachment={{ id: 'a1', path: '/pic.png' }} onRemove={vi.fn()} />)

    await user.click(screen.getByRole('button', { name: 'Preview image' }))
    expect(await screen.findByAltText('pic.png')).not.toBeNull()

    // Why: the shim toggle doubles as close; reopening must re-resolve per open.
    await user.click(screen.getByRole('button', { name: 'Preview image' }))
    expect(screen.queryByTestId('popover-content')).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Preview image' }))
    expect(await screen.findByAltText('pic.png')).not.toBeNull()
    expect(loadLocalImageAbsolutePathMock).toHaveBeenCalledTimes(2)
  })

  it('does not call the remove handler when opening the preview', async () => {
    const user = userEvent.setup()
    const onRemove = vi.fn()
    loadLocalImageAbsolutePathMock.mockResolvedValue('blob:x')
    render(<NativeChatImageAttachmentChip attachment={{ id: 'a1', path: '/x.png' }} onRemove={onRemove} />)

    await user.click(screen.getByRole('button', { name: 'Preview image' }))
    await screen.findByAltText('x.png')

    expect(onRemove).not.toHaveBeenCalled()
  })
})

beforeEach(() => {
  loadLocalImageAbsolutePathMock.mockReset()
})
