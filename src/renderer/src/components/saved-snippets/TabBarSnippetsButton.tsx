import { useCallback, useRef, useState } from 'react'
import { LibraryBig, Plus } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useAppStore } from '@/store'
import { translate } from '@/i18n/i18n'
import { useConfirmationDialog } from '@/components/confirmation-dialog-context'
import { useTabBarQuickCommandDrag } from '@/components/tab-bar/use-tab-bar-quick-command-drag'
import type { SavedSnippet } from '../../../../shared/saved-snippet-types'
import { SavedSnippetCard } from './SavedSnippetCard'
import { SavedSnippetDialog } from './SavedSnippetDialog'

type SnippetEditor = { mode: 'add' } | { mode: 'edit'; snippet: SavedSnippet }
const EMPTY_SNIPPETS: readonly SavedSnippet[] = []

export function TabBarSnippetsButton(): React.JSX.Element {
  const settings = useAppStore((state) => state.settings)
  const snippets = settings?.savedSnippets ?? EMPTY_SNIPPETS
  const updateSettings = useAppStore((state) => state.updateSettings)
  const confirm = useConfirmationDialog()
  const [open, setOpen] = useState(false)
  const [editor, setEditor] = useState<SnippetEditor | null>(null)
  const preserveDestinationFocusRef = useRef(false)
  const handleOpenChange = useCallback((next: boolean): void => setOpen(next), [])
  const drag = useTabBarQuickCommandDrag({
    onOpenChange: handleOpenChange,
    preserveDestinationFocusRef
  })
  const saveSnippet = (snippet: SavedSnippet): void => {
    const next = snippets.some((candidate) => candidate.id === snippet.id)
      ? snippets.map((candidate) => (candidate.id === snippet.id ? snippet : candidate))
      : [...snippets, snippet]
    void updateSettings({ savedSnippets: next })
    setEditor(null)
  }
  const deleteSnippet = async (snippet: SavedSnippet): Promise<void> => {
    const accepted = await confirm({
      title: translate('components.savedSnippets.delete.title', 'Delete "{{value0}}"?', {
        value0: snippet.name
      }),
      description: translate(
        'components.savedSnippets.delete.description',
        'This snippet card will be removed.'
      ),
      confirmLabel: translate('components.savedSnippets.delete.confirm', 'Delete'),
      confirmVariant: 'destructive'
    })
    if (accepted) {
      void updateSettings({
        savedSnippets: snippets.filter((candidate) => candidate.id !== snippet.id)
      })
    }
  }

  return (
    <>
      <DropdownMenu modal={false} open={open} onOpenChange={drag.handleOpenChange}>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="my-auto flex h-7 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-muted-foreground hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                aria-label={translate('components.savedSnippets.open', 'Open snippets')}
              >
                <LibraryBig className="size-3.5" />
                <span className="text-xs font-medium">
                  {translate('components.savedSnippets.label', 'Snippets')}
                </span>
              </button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent side="bottom" sideOffset={6}>
            {translate(
              'components.savedSnippets.tooltip',
              'Drag reusable text into Chat or Terminal'
            )}
          </TooltipContent>
        </Tooltip>
        <DropdownMenuContent
          align="end"
          side="bottom"
          sideOffset={6}
          className="w-[min(24rem,calc(100vw-2rem))] p-2"
          onInteractOutside={drag.preventOutsideInteraction}
          onCloseAutoFocus={(event) => {
            if (preserveDestinationFocusRef.current) {
              preserveDestinationFocusRef.current = false
              event.preventDefault()
            }
          }}
        >
          <div className="flex items-start justify-between gap-3 px-1 pb-2 pt-0.5">
            <div className="min-w-0">
              <div className="text-xs font-semibold text-foreground">
                {translate('components.savedSnippets.menu.title', 'Snippet cards')}
              </div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">
                {translate(
                  'components.savedSnippets.menu.hint',
                  'Drag a card into Chat or Terminal'
                )}
              </div>
            </div>
            <button
              type="button"
              onClick={() => {
                setOpen(false)
                setEditor({ mode: 'add' })
              }}
              className="flex shrink-0 items-center gap-1 rounded-md px-2 py-1.5 text-[11px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <Plus className="size-3.5" />
              {translate('components.savedSnippets.menu.add', 'New card')}
            </button>
          </div>
          {snippets.length === 0 ? (
            <button
              type="button"
              onClick={() => {
                setOpen(false)
                setEditor({ mode: 'add' })
              }}
              className="flex w-full flex-col items-center rounded-xl border border-dashed border-border px-4 py-8 text-center hover:bg-accent/40"
            >
              <LibraryBig className="size-5 text-muted-foreground" />
              <span className="mt-2 text-xs font-medium text-foreground">
                {translate('components.savedSnippets.menu.emptyTitle', 'Create your first card')}
              </span>
              <span className="mt-1 text-[11px] text-muted-foreground">
                {translate(
                  'components.savedSnippets.menu.emptyHint',
                  'Save any prompt, command, or reusable text.'
                )}
              </span>
            </button>
          ) : (
            <div className="scrollbar-sleek grid max-h-80 gap-2 overflow-y-auto pr-0.5">
              {snippets.map((snippet) => (
                <SavedSnippetCard
                  key={snippet.id}
                  snippet={snippet}
                  onEdit={() => {
                    setOpen(false)
                    setEditor({ mode: 'edit', snippet })
                  }}
                  onDelete={() => void deleteSnippet(snippet)}
                  onDragStarted={drag.startDrag}
                  onDragFinished={drag.finishDrag}
                />
              ))}
            </div>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {editor ? (
        <SavedSnippetDialog
          key={editor.mode === 'edit' ? editor.snippet.id : 'new'}
          mode={editor.mode}
          snippet={editor.mode === 'edit' ? editor.snippet : undefined}
          onClose={() => setEditor(null)}
          onSave={saveSnippet}
        />
      ) : null}
    </>
  )
}
