import { useEffect, useState, useTransition } from 'react'

/**
 * Why: zustand's external-store updates are urgent and cannot be deferred by
 * startTransition at the setter, so the shell defers the *rendered* view
 * instead: the previous page stays mounted while the next chunk loads.
 */
export function useStableActiveView(view: string): {
  displayedView: string
  navigationPending: boolean
} {
  const [displayedView, setDisplayedView] = useState(view)
  const [navigationPending, startNavigation] = useTransition()
  useEffect(() => {
    if (view === displayedView) {
      return
    }
    startNavigation(() => setDisplayedView(view))
  }, [view, displayedView])
  return { displayedView, navigationPending }
}
