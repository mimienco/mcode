import React from 'react'
import { Loader2, MessageSquarePlus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useQuickChatCreation } from '@/hooks/useQuickChatCreation'
import { translate } from '@/i18n/i18n'
import {
  handleRepoHeaderActionPointerDown,
  stopRepoHeaderKeyboardToggle
} from './header-event-guards'

export function QuickChatsCreateButton(): React.JSX.Element | null {
  const { createQuickChat, creating, available } = useQuickChatCreation()
  if (!available) {
    return null
  }

  const label = translate(
    'auto.components.sidebar.SidebarHeader.newChat',
    'New chat without a project'
  )
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          className="text-muted-foreground hover:bg-worktree-sidebar-accent/70 hover:text-worktree-sidebar-accent-foreground"
          data-repo-header-actions=""
          disabled={creating}
          aria-label={label}
          onPointerDown={handleRepoHeaderActionPointerDown}
          onKeyDown={stopRepoHeaderKeyboardToggle}
          onClick={(event) => {
            event.preventDefault()
            event.stopPropagation()
            createQuickChat()
          }}
        >
          {creating ? (
            <Loader2 className="animate-spin motion-reduce:animate-none" />
          ) : (
            <MessageSquarePlus />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={6}>
        {translate('auto.components.sidebar.SidebarHeader.newChatShort', 'New chat')}
      </TooltipContent>
    </Tooltip>
  )
}
