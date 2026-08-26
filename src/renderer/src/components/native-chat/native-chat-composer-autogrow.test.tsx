// @vitest-environment happy-dom

/** The composer grows with the draft up to 8 lines, then scrolls internally.
 *  Sizing is layout-driven (field-sizing + an lh-relative cap) rather than a JS
 *  measure pass, so these assert the class contract that produces it. happy-dom
 *  has no layout engine, so real pixel growth is covered by app validation. */

import { createRef } from 'react'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  QUICK_COMMAND_DRAG_MIME,
  finishQuickCommandDrag,
  writeQuickCommandDragData
} from '@/lib/quick-command-drag-data'

vi.mock('@/i18n/i18n', () => ({
  translate: (_key: string, fallback: string) => fallback
}))

vi.mock('./NativeChatComposerActions', () => ({
  NativeChatComposerActions: () => <div data-testid="composer-actions" />
}))

vi.mock('./NativeChatAutocompleteMenus', () => ({
  NativeChatMentionMenu: () => null,
  NativeChatPickerMenu: () => null
}))

import { NativeChatComposerField } from './NativeChatComposerField'

afterEach(() => cleanup())

function renderField(
  draft: string,
  onInsertQuickCommand = vi.fn(),
  disabled = false
): HTMLTextAreaElement {
  render(
    <NativeChatComposerField
      textareaRef={createRef<HTMLTextAreaElement>()}
      draft={draft}
      disabled={disabled}
      hasPty
      canSend
      autocomplete={{ mode: 'none' }}
      activeSuggestion={0}
      notice={null}
      imageAttachments={[]}
      fileReferences={[]}
      sendButtonDisabled={false}
      isWorking={false}
      attachDisabled={false}
      dictationDisabled={false}
      isDictating={false}
      isDictationHoldMode={false}
      onDraftChange={vi.fn()}
      onTextareaSelect={vi.fn()}
      onKeyDown={vi.fn()}
      onCompositionStart={vi.fn()}
      onCompositionEnd={vi.fn()}
      onPaste={vi.fn()}
      onInsertQuickCommand={onInsertQuickCommand}
      pickerListboxId="picker"
      onChoosePickerItem={vi.fn()}
      onRetrySkills={vi.fn()}
      mentionFiles={[]}
      mentionLoading={false}
      mentionLoadError={null}
      mentionTruncated={false}
      onChooseMention={vi.fn()}
      onRemoveImageAttachment={vi.fn()}
      onRemoveFileReference={vi.fn()}
      onAttach={vi.fn()}
      onDictationToggle={vi.fn()}
      onDictationHoldStart={vi.fn()}
      onDictationHoldEnd={vi.fn()}
      onSend={vi.fn()}
      sessionOptionsSurface={null}
      sessionOptionsSnapshot={[]}
    />
  )
  return screen.getByRole('textbox') as HTMLTextAreaElement
}

describe('native chat composer autogrow', () => {
  it('sizes the textarea from its content instead of staying at rows={2}', () => {
    expect(renderField('').className).toContain('[field-sizing:content]')
  })

  it('caps growth at 8 lines plus the py-1 padding box', () => {
    // 8lh tracks the rendered line-height, so the cap follows the text tokens
    // instead of a hardcoded pixel value like the old max-h-28 (112px).
    const textarea = renderField('a\n'.repeat(20))
    expect(textarea.className).toContain('max-h-[calc(8lh+0.5rem)]')
    expect(textarea.className).not.toContain('max-h-28')
  })

  it('keeps the sleek scrollbar for the overflow past the cap', () => {
    expect(renderField('a\n'.repeat(20)).className).toContain('scrollbar-sleek')
  })

  it('keeps a spacious writing area and the coarse-pointer touch target', () => {
    const textarea = renderField('')
    expect(textarea.className).toContain('min-h-14')
    expect(textarea.className).toContain('pointer-coarse:min-h-14')
    expect(document.querySelector('[data-native-chat-composer-surface]')).not.toBeNull()
  })

  it('does not pin an inline height that a resize could leave stale', () => {
    // A JS measure pass writes style.height and only re-measures on the next
    // value change, so a re-wrap from a window/pane resize would strand it.
    expect(renderField('a\n'.repeat(6)).style.height).toBe('')
  })

  it('accepts an internal quick command drop as unsent composer text', () => {
    const onInsertQuickCommand = vi.fn()
    renderField('draft', onInsertQuickCommand)
    const surface = document.querySelector('[data-native-chat-composer-surface]')
    if (!(surface instanceof HTMLElement)) {
      throw new Error('composer surface not found')
    }
    const dataTransfer = {
      types: [QUICK_COMMAND_DRAG_MIME],
      dropEffect: 'none',
      getData: (type: string) =>
        type === QUICK_COMMAND_DRAG_MIME
          ? JSON.stringify({ version: 1, label: 'Review', text: 'Review this diff' })
          : ''
    }

    expect(fireEvent.dragOver(surface, { dataTransfer })).toBe(false)
    expect(fireEvent.drop(surface, { dataTransfer })).toBe(false)
    expect(onInsertQuickCommand).toHaveBeenCalledWith('Review this diff')
  })

  it('owns the Electron dragend fallback and rejects it while disabled', () => {
    const enabledInsert = vi.fn()
    renderField('draft', enabledInsert)
    const surface = document.querySelector('[data-native-chat-composer-surface]')
    if (!(surface instanceof HTMLElement)) {
      throw new Error('composer surface not found')
    }
    vi.spyOn(document, 'elementFromPoint').mockReturnValue(surface)
    const values = new Map<string, string>()
    const dataTransfer = {
      effectAllowed: 'none',
      setData: (type: string, value: string) => values.set(type, value)
    } as unknown as DataTransfer
    writeQuickCommandDragData(dataTransfer, 'Review', 'Review this diff')

    expect(finishQuickCommandDrag(10, 20)).toBe(true)
    expect(enabledInsert).toHaveBeenCalledWith('Review this diff')

    cleanup()
    const disabledInsert = vi.fn()
    renderField('draft', disabledInsert, true)
    const disabledSurface = document.querySelector('[data-native-chat-composer-surface]')
    if (!(disabledSurface instanceof HTMLElement)) {
      throw new Error('disabled composer surface not found')
    }
    vi.mocked(document.elementFromPoint).mockReturnValue(disabledSurface)
    writeQuickCommandDragData(dataTransfer, 'Review', 'Review this diff')

    expect(finishQuickCommandDrag(10, 20)).toBe(false)
    expect(disabledInsert).not.toHaveBeenCalled()
  })
})
