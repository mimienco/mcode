import { useCallback, useRef, type RefObject } from 'react'

export function useTabBarQuickCommandDrag({
  onOpenChange,
  preserveDestinationFocusRef
}: {
  onOpenChange: (open: boolean) => void
  preserveDestinationFocusRef: RefObject<boolean>
}): {
  finishDrag: (dropped: boolean) => void
  handleOpenChange: (open: boolean) => void
  preventOutsideInteraction: (event: { preventDefault: () => void }) => void
  startDrag: () => void
} {
  const dragInProgressRef = useRef(false)
  const handleOpenChange = useCallback(
    (open: boolean): void => {
      if (!open && dragInProgressRef.current) {
        return
      }
      onOpenChange(open)
    },
    [onOpenChange]
  )
  const startDrag = useCallback((): void => {
    dragInProgressRef.current = true
  }, [])
  const finishDrag = useCallback(
    (dropped: boolean): void => {
      dragInProgressRef.current = false
      preserveDestinationFocusRef.current = dropped
      onOpenChange(false)
    },
    [onOpenChange, preserveDestinationFocusRef]
  )
  const preventOutsideInteraction = useCallback((event: { preventDefault: () => void }): void => {
    if (dragInProgressRef.current) {
      event.preventDefault()
    }
  }, [])
  return { finishDrag, handleOpenChange, preventOutsideInteraction, startDrag }
}
