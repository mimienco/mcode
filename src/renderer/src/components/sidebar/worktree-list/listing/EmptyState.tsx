import React from 'react'
import { CircleX, GitBranchPlus, Loader2, MessageSquarePlus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { translate } from '@/i18n/i18n'
import { useAppStore } from '@/store'
import { useQuickChatCreation } from '@/hooks/useQuickChatCreation'

export function SidebarWorktreeListEmptyState({
  hasFilters,
  onClearFilters
}: {
  hasFilters: boolean
  onClearFilters: () => void
}): React.JSX.Element {
  const openModal = useAppStore((s) => s.openModal)
  const { createQuickChat, creating, available: quickChatAvailable } = useQuickChatCreation()

  if (!hasFilters) {
    // Test Bench: a genuine empty bench gets a call-to-action, not a dead end.
    return (
      <div
        data-worktree-sidebar-container
        data-contextual-tour-target="workspace-list"
        className="relative min-h-0 flex-1"
      >
        <div className="worktree-sidebar-scrollbar flex h-full flex-col overflow-y-auto overflow-x-hidden pl-1 scrollbar-sleek pt-px">
          <div className="mx-2 mt-3 flex flex-col items-center gap-2.5 rounded-lg border border-dashed border-border/80 px-4 py-7 text-center">
            {quickChatAvailable ? (
              <MessageSquarePlus className="size-5 text-muted-foreground/70" />
            ) : (
              <GitBranchPlus className="size-5 text-muted-foreground/70" />
            )}
            <div className="space-y-1">
              <p className="text-xs font-medium text-foreground">
                {translate(
                  'auto.components.sidebar.WorktreeList.b7acbf038b',
                  'No workspaces found'
                )}
              </p>
              <p className="text-[11px] leading-snug text-muted-foreground">
                {quickChatAvailable
                  ? translate(
                      'auto.components.sidebar.WorktreeList.quickChatEmptyHint',
                      'Start a chat now. Add a project when you need repository context.'
                    )
                  : translate(
                      'auto.components.sidebar.WorktreeList.emptyHint',
                      'Spin up your first agent workspace.'
                    )}
              </p>
            </div>
            <Button
              variant={quickChatAvailable ? 'default' : 'secondary'}
              size="xs"
              disabled={quickChatAvailable && creating}
              onClick={() =>
                quickChatAvailable
                  ? createQuickChat()
                  : openModal('new-workspace-composer', { telemetrySource: 'unknown' })
              }
              className="gap-1.5 border border-border/80 text-[11px]"
            >
              {quickChatAvailable && creating ? (
                <Loader2 className="size-3 animate-spin" />
              ) : quickChatAvailable ? (
                <MessageSquarePlus className="size-3" />
              ) : (
                <GitBranchPlus className="size-3" />
              )}
              {quickChatAvailable
                ? translate('auto.components.Landing.newChat', 'New Chat')
                : translate('auto.components.Landing.76a95f7f47', 'Create')}
            </Button>
            {quickChatAvailable ? (
              <Button
                variant="link"
                size="xs"
                onClick={() => openModal('new-workspace-composer', { telemetrySource: 'unknown' })}
                className="h-auto px-0 text-[11px] text-muted-foreground hover:text-foreground"
              >
                {translate('auto.components.sidebar.WorktreeList.newWorkspace', 'New workspace')}
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div
      data-worktree-sidebar-container
      data-contextual-tour-target="workspace-list"
      className="relative min-h-0 flex-1"
    >
      <div className="worktree-sidebar-scrollbar flex h-full flex-col overflow-y-auto overflow-x-hidden pl-1 scrollbar-sleek pt-px">
        <div className="flex flex-col items-center gap-2 px-4 py-6 text-center text-[11px] text-muted-foreground">
          <span>
            {translate('auto.components.sidebar.WorktreeList.b7acbf038b', 'No workspaces found')}
          </span>
          {hasFilters && (
            <Button
              variant="secondary"
              size="xs"
              onClick={onClearFilters}
              className="gap-1.5 border border-border/80 text-[11px]"
            >
              <CircleX className="size-3.5" />
              {translate('auto.components.sidebar.WorktreeList.370c6a55dd', 'Clear Filters')}
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}
