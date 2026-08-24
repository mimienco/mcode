import { useEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { Image as ImageIcon, X } from 'lucide-react'
import { Dialog, DialogClose, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Popover, PopoverArrow, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { translate } from '@/i18n/i18n'
import { loadLocalImageAbsolutePath } from '@/components/editor/useLocalImageSrc'

// Why: transcript pages can hold many chips; thumbnails only pay their IPC read
// once scrolled near, and environments without IntersectionObserver stay eager.
function useVisibleOnce(ref: RefObject<HTMLElement | null>): boolean {
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const element = ref.current
    if (!element || typeof IntersectionObserver === 'undefined') {
      setVisible(true)
      return
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        setVisible(true)
        observer.disconnect()
      }
    })
    observer.observe(element)
    return () => observer.disconnect()
    // Why: the ref object is stable for a mounted chip; once-semantics intended.
  }, [ref])
  return visible
}

export type NativeChatImagePreviewChipProps = {
  /** Local absolute path: resolved through the image cache when opened. */
  path?: string
  /** Direct src (data:/https:/blob:) used as-is when no path is given. */
  src?: string
  label: string
  /** Why: opt-in so tests and minimal call sites keep the plain icon chip. */
  showThumbnail?: boolean
  /** Trailing content rendered inside the same pill (e.g. a remove button). */
  children?: ReactNode
}

/** Image chip that opens a small anchored preview; shared by composer and transcript. */
export function NativeChatImagePreviewChip(
  props: NativeChatImagePreviewChipProps
): React.JSX.Element {
  const { path, src, label, showThumbnail = false, children } = props
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const visible = useVisibleOnce(triggerRef)
  const [open, setOpen] = useState(false)
  const [lightboxOpen, setLightboxOpen] = useState(false)
  const [previewUrl, setPreviewUrl] = useState<string | null | undefined>(undefined)
  const [thumbUrl, setThumbUrl] = useState<string | null>(null)

  useEffect(() => {
    if (!showThumbnail || !visible) {
      return
    }
    // Why: the popover loader shares the same blob cache, so the open-read is a hit.
    const direct = path ? loadLocalImageAbsolutePath(path) : Promise.resolve(src ?? null)
    void direct.then((url) => {
      if (url) {
        setThumbUrl(url)
      }
    })
  }, [showThumbnail, visible, path, src])

  // Why: undefined = not yet loaded for this open, null = load failed.
  const handleOpenChange = (nextOpen: boolean): void => {
    setOpen(nextOpen)
    if (nextOpen && path) {
      setPreviewUrl(undefined)
      void loadLocalImageAbsolutePath(path).then((url) => setPreviewUrl(url))
    }
  }

  const resolvedSrc = path ? previewUrl : src

  return (
    <div className="flex max-w-full items-center gap-1.5 rounded-md border border-border bg-background px-2 py-1 text-xs text-muted-foreground">
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <button
            ref={triggerRef}
            type="button"
            aria-label={translate('components.native-chat.composer.previewAttachment', 'Preview image')}
            title={path ?? src}
            className="flex min-w-0 items-center gap-1.5 rounded-sm text-xs text-muted-foreground transition-colors hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-zoom-in"
          >
            {thumbUrl ? (
              <img
                src={thumbUrl}
                alt=""
                className="size-5 shrink-0 rounded-sm object-cover outline outline-1 outline-border/60"
              />
            ) : (
              <ImageIcon className="size-3.5 shrink-0" />
            )}
            <span className="max-w-56 truncate">{label}</span>
          </button>
        </PopoverTrigger>
        <PopoverContent side="top" align="start" className="w-64 p-1.5">
          {resolvedSrc === undefined ? (
            <div className="h-40 w-full animate-pulse rounded bg-muted" />
          ) : resolvedSrc === null ? (
            <div className="flex h-40 w-full items-center justify-center text-xs text-muted-foreground">
              {translate('components.native-chat.composer.previewUnavailable', 'Preview unavailable')}
            </div>
          ) : (
            // Why: blob URL lifecycle is owned by the image cache module; never revoke here.
            <button
              type="button"
              aria-label={translate(
                'auto.components.sidebar.MarkdownImageLightbox.expand',
                'Expand image'
              )}
              className="block w-full cursor-zoom-in rounded outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onClick={() => {
                // Why: the dialog owns focus; the popover would fight it underneath.
                setOpen(false)
                setLightboxOpen(true)
              }}
            >
              <img src={resolvedSrc} alt={label} className="max-h-64 w-full rounded object-contain" />
            </button>
          )}
          <PopoverArrow className="native-chat-picker-arrow" width={16} height={8} />
        </PopoverContent>
      </Popover>
      {lightboxOpen && typeof resolvedSrc === 'string' ? (
        <Dialog open={lightboxOpen} onOpenChange={setLightboxOpen}>
          <DialogContent
            aria-describedby={undefined}
            showCloseButton={false}
            className="flex h-[90dvh] w-[90vw] max-w-[90vw] flex-col gap-0 overflow-hidden p-0 sm:max-w-[90vw]"
          >
            <DialogTitle className="sr-only">{label}</DialogTitle>
            <div className="flex shrink-0 items-center justify-between border-b border-border px-3 py-2">
              <span className="min-w-0 truncate text-sm font-medium text-foreground">{label}</span>
              <DialogClose asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={translate(
                    'auto.components.sidebar.MarkdownImageLightbox.close',
                    'Close'
                  )}
                >
                  <X className="size-4" />
                </Button>
              </DialogClose>
            </div>
            <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto bg-muted/20 p-4 scrollbar-editor">
              <img src={resolvedSrc} alt={label} className="max-h-full max-w-full rounded-md object-contain" />
            </div>
          </DialogContent>
        </Dialog>
      ) : null}
      {children}
    </div>
  )
}
