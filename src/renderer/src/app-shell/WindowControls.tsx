import { useEffect, useState } from 'react'
import { translate } from '@/i18n/i18n'
import type { DesktopWindowCapability } from '@/desktop/desktop-client'
import { desktopWindowClient } from '@/desktop/runtime-desktop-window-client'

type WindowControlsCapability = Pick<
  DesktopWindowCapability,
  'minimize' | 'toggleMaximize' | 'isMaximized' | 'onMaximizedChanged' | 'requestClose'
>

type WindowControlsProps = {
  desktopWindow?: WindowControlsCapability
}

// Why: Windows and Linux both remove the native title bar, so we render our own min/max/close buttons (Fluent/Win11-style SVGs).
export function WindowControls({
  desktopWindow = desktopWindowClient
}: WindowControlsProps = {}): React.JSX.Element {
  const [maximized, setMaximized] = useState(false)
  useEffect(() => {
    // Why: maximize-changed only fires on transitions; seed from main on mount so a startup-maximized window shows the right icon.
    let cancelled = false
    let unsubscribe: (() => void) | undefined
    void desktopWindow.isMaximized().then((value) => {
      if (!cancelled) {
        setMaximized(value)
      }
    })
    void desktopWindow
      .onMaximizedChanged((value) => {
        if (!cancelled) {
          setMaximized(value)
        }
      })
      .then((stop) => {
        if (cancelled) {
          stop()
        } else {
          unsubscribe = stop
        }
      })
    return () => {
      cancelled = true
      unsubscribe?.()
    }
  }, [desktopWindow])
  return (
    <div className="window-controls">
      <button
        className="window-controls-btn"
        aria-label={translate('auto.App.bbb7f90669', 'Minimize')}
        onClick={() => void desktopWindow.minimize()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <path d="M0 5h10v1H0z" fill="currentColor" />
        </svg>
      </button>
      <button
        className="window-controls-btn"
        aria-label={
          maximized
            ? translate('auto.App.66f0a552e5', 'Restore')
            : translate('auto.App.c9d6f98459', 'Maximize')
        }
        onClick={() => void desktopWindow.toggleMaximize()}
      >
        {maximized ? (
          // Restore icon (two overlapping squares)
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M2 0v2H0v8h8V8h2V0H2zm6 9H1V3h7v6zM9 7H8V2H3V1h6v6z" fill="currentColor" />
          </svg>
        ) : (
          // Maximize icon (single square outline)
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M0 0v10h10V0H0zm9 9H1V1h8v8z" fill="currentColor" />
          </svg>
        )}
      </button>
      <button
        className="window-controls-btn window-controls-close"
        aria-label={translate('auto.App.e960d18540', 'Close')}
        // Why: route close through the desktop runtime so Electron keeps its main-process guard and Tauri can attach its equivalent lifecycle policy at this boundary.
        onClick={() => void desktopWindow.requestClose()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <path d="M1 0L0 1l4 4-4 4 1 1 4-4 4 4 1-1-4-4 4-4-1-1-4 4-4-4z" fill="currentColor" />
        </svg>
      </button>
    </div>
  )
}
