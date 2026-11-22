import { useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { translate } from '@/i18n/i18n'
import { createBrowserUuid } from '@/lib/browser-uuid'
import type { SavedSnippet } from '../../../../shared/saved-snippet-types'
import {
  MAX_SAVED_SNIPPET_NAME_LENGTH,
  MAX_SAVED_SNIPPET_TEXT_LENGTH
} from '../../../../shared/saved-snippets'

export function SavedSnippetDialog({
  mode,
  snippet,
  onClose,
  onSave
}: {
  mode: 'add' | 'edit'
  snippet?: SavedSnippet
  onClose: () => void
  onSave: (snippet: SavedSnippet) => void
}): React.JSX.Element {
  const [name, setName] = useState(snippet?.name ?? '')
  const [text, setText] = useState(snippet?.text ?? '')
  const canSave = Boolean(name.trim() && text.trim())
  const save = (): void => {
    if (!canSave) {
      return
    }
    onSave({
      id: snippet?.id ?? `snippet-${createBrowserUuid()}`,
      name: name.trim(),
      text: text.trimEnd()
    })
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {mode === 'edit'
              ? translate('components.savedSnippets.dialog.editTitle', 'Edit card')
              : translate('components.savedSnippets.dialog.addTitle', 'New snippet card')}
          </DialogTitle>
          <DialogDescription>
            {translate(
              'components.savedSnippets.dialog.description',
              'This text can be dragged into either Chat or Terminal.'
            )}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-4 py-1">
          <div className="grid gap-2">
            <Label htmlFor="saved-snippet-name">
              {translate('components.savedSnippets.dialog.name', 'Name')}
            </Label>
            <Input
              id="saved-snippet-name"
              autoFocus
              maxLength={MAX_SAVED_SNIPPET_NAME_LENGTH}
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={translate(
                'components.savedSnippets.dialog.namePlaceholder',
                'Example: Review Git changes'
              )}
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="saved-snippet-text">
              {translate('components.savedSnippets.dialog.text', 'Text')}
            </Label>
            <textarea
              id="saved-snippet-text"
              maxLength={MAX_SAVED_SNIPPET_TEXT_LENGTH}
              value={text}
              onChange={(event) => setText(event.target.value)}
              placeholder={translate(
                'components.savedSnippets.dialog.textPlaceholder',
                'Write a prompt, command, or any reusable text…'
              )}
              className="min-h-36 resize-y rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            />
          </div>
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            {translate('components.savedSnippets.dialog.cancel', 'Cancel')}
          </Button>
          <Button type="button" disabled={!canSave} onClick={save}>
            {translate('components.savedSnippets.dialog.save', 'Save card')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
