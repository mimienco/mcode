import { ChevronDown, PanelLeftClose, PanelLeftOpen } from 'lucide-react'
import logo from '../../../../resources/logo.svg'
import { translate } from '@/i18n/i18n'
import { ShortcutKeyCombo } from '@/components/ShortcutKeyCombo'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger
} from '@/components/ui/context-menu'
import { useShortcutKeyDetails } from '../hooks/useShortcutLabel'
import { useAppStore } from '../store'
import { hasCustomTitleBar, isMac } from './app-window-chrome'
import type { AppChromeLayout } from './use-app-chrome-layout'

/**
 * The titlebar's left cluster: window chrome padding, app name, sidebar toggle, and the
 * worktree back/forward pair. Shared by the full-width titlebar and the sidebar-width left
 * header so the agent badge popover isn't duplicated.
 */
export function TitlebarLeftControls({ layout }: { layout: AppChromeLayout }): React.JSX.Element {
  const toggleSidebar = useAppStore((s) => s.toggleSidebar)
  const updateSettings = useAppStore((s) => s.updateSettings)
  const leftSidebarShortcut = useShortcutKeyDetails('sidebar.left.toggle')
  const sidebarActionLabel = layout.sidebarOpen
    ? translate('auto.App.a27f8dd81a', 'Hide sidebar')
    : translate('auto.App.18d43bd86c', 'Show sidebar')

  return (
    // Why: measure the full collapsed launcher so tab strips reserve exactly the chrome that floats above them.
    // Why: collapsed mode floats in a w-0 wrapper; w-max stops Windows Chromium from shrinking the app name to one glyph.
    <div
      ref={layout.titlebarLeftControlsRef}
      className={`flex h-full shrink-0 items-center${
        layout.leftTitlebarChromeLayout.isFloating ? ' w-max' : ' w-full'
      }`}
    >
      <div
        className={`flex h-full items-center${layout.sidebarOpen ? ' w-full justify-between' : ''}`}
      >
        {isMac && !layout.isFullScreen ? (
          <div className="titlebar-traffic-light-pad" />
        ) : hasCustomTitleBar ? (
          /* Why: one branded control communicates app-level scope without the ambiguous
             static-logo-plus-overflow pair that survived sidebar collapse. */
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                className="titlebar-app-menu-button"
                aria-label={translate('auto.App.8b0b8eb54f', 'Application menu')}
                onClick={() => window.api.ui.popupMenu()}
              >
                <img src={logo} alt="" aria-hidden className="titlebar-logo" />
                <ChevronDown className="titlebar-app-menu-chevron" aria-hidden />
              </button>
            </TooltipTrigger>
            <TooltipContent side="bottom" sideOffset={6}>
              {translate('auto.App.8b0b8eb54f', 'Application menu')}
            </TooltipContent>
          </Tooltip>
        ) : (
          <div className="pl-2" />
        )}
        {layout.showSidebar && !hasCustomTitleBar && layout.showTitlebarAppName && (
          <ContextMenu>
            <ContextMenuTrigger asChild>
              <div
                className="titlebar-app-name"
                aria-label={translate('auto.App.5096cbbc86', 'MCode')}
              >
                <span className="titlebar-app-name-main">
                  {translate('auto.App.5096cbbc86', 'MCode')}
                </span>
              </div>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem
                onSelect={() => {
                  void updateSettings({ showTitlebarAppName: false })
                }}
              >
                {translate('auto.App.e81217c1b7', 'Hide App Name')}
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
        )}
        {layout.showSidebar && (
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                className="sidebar-toggle"
                onClick={toggleSidebar}
                aria-label={sidebarActionLabel}
                aria-expanded={layout.sidebarOpen}
                aria-controls="worktree-sidebar"
              >
                {layout.sidebarOpen ? <PanelLeftClose size={16} /> : <PanelLeftOpen size={16} />}
              </button>
            </TooltipTrigger>
            <TooltipContent side="bottom" sideOffset={6}>
              <span className="flex items-center gap-2">
                <span>{sidebarActionLabel}</span>
                {leftSidebarShortcut.keys.length > 0 ? (
                  <ShortcutKeyCombo
                    keys={leftSidebarShortcut.keys}
                    doubleTap={leftSidebarShortcut.doubleTap}
                    keyCapClassName="min-w-4 border-background/25 bg-background/10 px-1 py-0 text-[10px] text-background shadow-none"
                    separatorClassName="text-[10px] text-background/70"
                  />
                ) : null}
              </span>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
    </div>
  )
}
