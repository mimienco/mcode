import type {
  ClipboardEventHandler,
  CompositionEventHandler,
  KeyboardEventHandler,
  RefObject
} from 'react'
import { CircleAlert } from 'lucide-react'
import { translate } from '@/i18n/i18n'
import { cn } from '@/lib/utils'
import { NATIVE_FILE_DROP_TARGET } from '../../../../shared/native-file-drop'
import type { ComposerAutocomplete, NativeChatPickerItem } from './native-chat-composer-state'
import { NativeChatMentionMenu, NativeChatPickerMenu } from './NativeChatAutocompleteMenus'
import type { NativeChatMentionFile } from './use-native-chat-mention-files'
import { NativeChatComposerActions } from './NativeChatComposerActions'
import { NativeChatFileReferenceChip } from './NativeChatFileReferenceChip'
import { NativeChatImageAttachmentChip } from './NativeChatImageAttachmentChip'
import { nativeChatComposerPlaceholder } from './native-chat-composer-target'
import type { NativeChatComposerFileReference } from './use-native-chat-composer-attachments'
import type {
  SessionOptionDescriptor,
  SessionOptionsSurface
} from '../../../../shared/native-chat-session-options'

export type NativeChatComposerFieldProps = {
  textareaRef: RefObject<HTMLTextAreaElement | null>
  draft: string
  disabled: boolean
  hasPty: boolean
  canSend: boolean
  autocomplete: ComposerAutocomplete
  activeSuggestion: number
  notice: string | null
  imageAttachments: readonly NativeChatComposerImageAttachment[]
  fileReferences: readonly NativeChatComposerFileReference[]
  sendButtonDisabled: boolean
  isWorking: boolean
  attachDisabled: boolean
  dictationDisabled: boolean
  isDictating: boolean
  isDictationHoldMode: boolean
  onDraftChange: (value: string, element: HTMLTextAreaElement) => void
  onTextareaSelect: (element: HTMLTextAreaElement) => void
  onKeyDown: KeyboardEventHandler<HTMLTextAreaElement>
  onCompositionStart: CompositionEventHandler<HTMLTextAreaElement>
  onCompositionEnd: CompositionEventHandler<HTMLTextAreaElement>
  onPaste: ClipboardEventHandler<HTMLTextAreaElement>
  pickerListboxId: string
  onChoosePickerItem: (item: NativeChatPickerItem) => void
  onRetrySkills: () => void
  mentionFiles: readonly NativeChatMentionFile[]
  mentionLoading: boolean
  mentionLoadError: string | null
  mentionTruncated: boolean
  onChooseMention: (path: string) => void
  onRemoveImageAttachment: (id: string) => void
  onRemoveFileReference: (id: string) => void
  onAttach: () => void
  onDictationToggle: () => void
  onDictationHoldStart: () => void
  onDictationHoldEnd: () => void
  onSend: () => void
  onStop?: () => void
  sessionOptionsSurface: SessionOptionsSurface | null
  sessionOptionsSnapshot: SessionOptionDescriptor[]
}

export type NativeChatComposerImageAttachment = {
  id: string
  path: string
}

export function NativeChatComposerField({
  textareaRef,
  draft,
  disabled,
  hasPty,
  canSend,
  autocomplete,
  activeSuggestion,
  notice,
  imageAttachments,
  fileReferences,
  sendButtonDisabled,
  isWorking,
  attachDisabled,
  dictationDisabled,
  isDictating,
  isDictationHoldMode,
  onDraftChange,
  onTextareaSelect,
  onKeyDown,
  onCompositionStart,
  onCompositionEnd,
  onPaste,
  pickerListboxId,
  onChoosePickerItem,
  onRetrySkills,
  mentionFiles,
  mentionLoading,
  mentionLoadError,
  mentionTruncated,
  onChooseMention,
  onRemoveImageAttachment,
  onRemoveFileReference,
  onAttach,
  onDictationToggle,
  onDictationHoldStart,
  onDictationHoldEnd,
  onSend,
  onStop,
  sessionOptionsSurface,
  sessionOptionsSnapshot
}: NativeChatComposerFieldProps): React.JSX.Element {
  const activeOptionCount =
    autocomplete.mode === 'mention'
      ? mentionFiles.length
      : 'items' in autocomplete
        ? autocomplete.items.length
        : 0
  return (
    <div className="shrink-0 border-t border-border/40 bg-background/95">
      {/* Extra bottom padding keeps the input box off the window rim. */}
      <div className="px-3 pt-3 pb-4 sm:px-5">
        <div className="relative mx-auto w-full max-w-3xl">
          {autocomplete.mode === 'slash' || autocomplete.mode === 'skill' ? (
            <NativeChatPickerMenu
              autocomplete={autocomplete}
              activeIndex={activeSuggestion}
              listboxId={pickerListboxId}
              onChoose={onChoosePickerItem}
              onRetry={onRetrySkills}
            />
          ) : null}
          {autocomplete.mode === 'mention' ? (
            <NativeChatMentionMenu
              query={autocomplete.query}
              files={mentionFiles}
              loading={mentionLoading}
              loadError={mentionLoadError}
              truncated={mentionTruncated}
              activeIndex={activeSuggestion}
              listboxId={pickerListboxId}
              onChoose={onChooseMention}
            />
          ) : null}
          {notice ? (
            <div
              role="status"
              aria-live="polite"
              className="mb-1.5 flex items-center gap-1.5 text-xs text-muted-foreground"
            >
              <CircleAlert className="size-3.5 shrink-0" aria-hidden="true" />
              <span>{notice}</span>
            </div>
          ) : null}
          <div
            data-native-file-drop-target={NATIVE_FILE_DROP_TARGET.composer}
            data-native-chat-composer-surface
            className={cn(
              'rounded-xl border border-border/80 bg-card/80 p-2 transition-colors',
              'focus-within:border-ring/55'
            )}
          >
            {fileReferences.length > 0 || imageAttachments.length > 0 ? (
              <div className="mb-2 flex flex-wrap gap-1.5 px-1">
                {fileReferences.map((reference) => (
                  <NativeChatFileReferenceChip
                    key={reference.id}
                    reference={reference}
                    onRemove={onRemoveFileReference}
                  />
                ))}
                {imageAttachments.map((attachment) => (
                  <NativeChatImageAttachmentChip
                    key={attachment.id}
                    attachment={attachment}
                    onRemove={onRemoveImageAttachment}
                  />
                ))}
              </div>
            ) : null}
            <textarea
              ref={textareaRef}
              value={draft}
              disabled={disabled}
              rows={2}
              onChange={(e) => onDraftChange(e.target.value, e.currentTarget)}
              onKeyDown={onKeyDown}
              onCompositionStart={onCompositionStart}
              onCompositionEnd={onCompositionEnd}
              onPaste={onPaste}
              onSelect={(e) => onTextareaSelect(e.currentTarget)}
              aria-label={translate(
                'components.native-chat.composer.inputLabel',
                'Message to agent'
              )}
              aria-expanded={autocomplete.mode !== 'none'}
              aria-controls={autocomplete.mode !== 'none' ? pickerListboxId : undefined}
              aria-activedescendant={
                autocomplete.mode !== 'none' && activeOptionCount > 0
                  ? `${pickerListboxId}-option-${Math.min(activeSuggestion, activeOptionCount - 1)}`
                  : undefined
              }
              placeholder={nativeChatComposerPlaceholder(hasPty, canSend)}
              // Why: coarse-pointer min-height follows the app's touch target convention.
              // field-sizing:content grows the field with the draft; the 8lh cap (plus
              // py-1) turns further growth into internal scrolling, and scrollbar-sleek
              // keeps that gutter off the heavy native scrollbar. Both are layout-driven,
              // so re-wrap on window/pane resize is handled without a measure pass.
              className={cn(
                'scrollbar-sleek min-h-14 w-full resize-none bg-transparent px-2.5 py-2 text-sm leading-6 outline-none pointer-coarse:min-h-14',
                '[field-sizing:content] max-h-[calc(8lh+0.5rem)]',
                'placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50'
              )}
            />
            <div className="mt-1 flex flex-wrap items-center gap-2 border-t border-border/50 pt-2">
              <NativeChatComposerActions
                attachDisabled={attachDisabled}
                dictationDisabled={dictationDisabled}
                sendDisabled={sendButtonDisabled}
                isWorking={isWorking}
                isDictating={isDictating}
                isDictationHoldMode={isDictationHoldMode}
                onAttach={onAttach}
                onDictationToggle={onDictationToggle}
                onDictationHoldStart={onDictationHoldStart}
                onDictationHoldEnd={onDictationHoldEnd}
                onSend={onSend}
                onStop={onStop}
                sessionOptionsSurface={sessionOptionsSurface}
                sessionOptionsSnapshot={sessionOptionsSnapshot}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
