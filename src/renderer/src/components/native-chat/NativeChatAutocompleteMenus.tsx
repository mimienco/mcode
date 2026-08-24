import { memo, useEffect, useRef } from 'react'
import { Loader2, Package, RotateCcw } from 'lucide-react'
import { translate } from '@/i18n/i18n'
import { cn } from '@/lib/utils'
import { getFileTypeIcon, getFileTypeIconColorClass } from '@/lib/file-type-icons'
import type { SkillSourceKind } from '../../../../shared/skills'
import type { ComposerAutocomplete, NativeChatPickerItem } from './native-chat-composer-state'
import type { NativeChatMentionFile } from './use-native-chat-mention-files'

export const NativeChatPickerMenu = memo(function NativeChatPickerMenu({
  autocomplete,
  activeIndex,
  listboxId,
  onChoose,
  onRetry
}: {
  autocomplete: Extract<ComposerAutocomplete, { mode: 'slash' | 'skill' }>
  activeIndex: number
  listboxId: string
  onChoose: (item: NativeChatPickerItem) => void
  onRetry: () => void
}): React.JSX.Element {
  const activeItemRef = useRef<HTMLButtonElement | null>(null)
  const commands = autocomplete.items.filter(
    (item): item is Extract<NativeChatPickerItem, { kind: 'command' }> => item.kind === 'command'
  )
  const skills = autocomplete.items.filter(
    (item): item is Extract<NativeChatPickerItem, { kind: 'skill' }> => item.kind === 'skill'
  )

  useEffect(() => {
    activeItemRef.current?.scrollIntoView({ block: 'nearest' })
  }, [activeIndex, autocomplete.items])

  const hasSkillStatus =
    autocomplete.skillStatus === 'loading' || autocomplete.skillStatus === 'error'
  const showCommandsHeading = autocomplete.grouped && commands.length > 0
  const showSkillsHeading = autocomplete.grouped && (skills.length > 0 || hasSkillStatus)
  const noMatches =
    autocomplete.skillStatus === 'ready' && commands.length === 0 && skills.length === 0
  const emptyText = noMatches ? getPickerEmptyText(autocomplete) : null
  const collision = commands.find((item) => item.skillCollision)
  const duplicate = skills.find((item) => item.sources.length > 1)

  let optionIndex = 0
  return (
    <div
      id={listboxId}
      role="listbox"
      className="scrollbar-sleek absolute bottom-full left-0 right-0 z-20 mb-1 max-h-72 overflow-y-auto rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-[0_10px_24px_rgba(0,0,0,0.18)]"
    >
      {showCommandsHeading ? <PickerGroupHeading kind="commands" /> : null}
      {commands.map((item) => {
        const index = optionIndex++
        return (
          <PickerOption
            key={item.id}
            item={item}
            prefix={autocomplete.prefix}
            index={index}
            activeIndex={activeIndex}
            listboxId={listboxId}
            activeItemRef={activeItemRef}
            onChoose={onChoose}
          />
        )
      })}
      {showSkillsHeading ? <PickerGroupHeading kind="skills" /> : null}
      {autocomplete.skillStatus === 'loading' ? (
        <PickerStatus>
          <Loader2 className="size-3.5 animate-spin" />
          {translate('components.native-chat.composer.loadingSkills', 'Loading skills...')}
        </PickerStatus>
      ) : null}
      {autocomplete.skillStatus === 'error' ? (
        <PickerStatus>
          <span className="min-w-0 flex-1">
            {autocomplete.skillErrorKind === 'unavailable'
              ? translate(
                  'components.native-chat.composer.skillsUnavailableHost',
                  'Skills are unavailable for this host'
                )
              : translate(
                  'components.native-chat.composer.skillsLoadFailed',
                  'Could not load skills from this host'
                )}
          </span>
          {autocomplete.skillErrorKind !== 'unavailable' ? (
            <button
              type="button"
              onPointerDown={(event) => event.preventDefault()}
              onClick={onRetry}
              className="flex shrink-0 items-center gap-1 rounded-sm px-1.5 py-0.5 text-foreground hover:bg-accent hover:text-accent-foreground"
            >
              <RotateCcw className="size-3" />
              {translate('components.native-chat.composer.retrySkills', 'Retry')}
            </button>
          ) : null}
        </PickerStatus>
      ) : null}
      {skills.map((item) => {
        const index = optionIndex++
        return (
          <PickerOption
            key={item.id}
            item={item}
            prefix={autocomplete.prefix}
            index={index}
            activeIndex={activeIndex}
            listboxId={listboxId}
            activeItemRef={activeItemRef}
            onChoose={onChoose}
          />
        )
      })}
      {noMatches ? <PickerStatus>{emptyText}</PickerStatus> : null}
      <div aria-live="polite" className="sr-only">
        {autocomplete.skillStatus === 'loading'
          ? translate('components.native-chat.composer.loadingSkills', 'Loading skills...')
          : autocomplete.skillStatus === 'error'
            ? translate(
                'components.native-chat.composer.skillsLoadFailed',
                'Could not load skills from this host'
              )
            : emptyText
              ? emptyText
              : autocomplete.skillsEnabled
                ? [
                    translate('components.native-chat.composer.skillsLoaded', 'Skills loaded'),
                    collision
                      ? getPickerAnnotation(collision)
                      : duplicate
                        ? getPickerAnnotation(duplicate)
                        : null
                  ]
                    .filter(Boolean)
                    .join('. ')
                : ''}
      </div>
    </div>
  )
})

function getPickerEmptyText(
  autocomplete: Extract<ComposerAutocomplete, { mode: 'slash' | 'skill' }>
): string {
  if (autocomplete.mode === 'skill' || !autocomplete.commandsEnabled) {
    return translate('components.native-chat.composer.noSkills', 'No matching skills')
  }
  if (autocomplete.skillsEnabled) {
    return translate(
      'components.native-chat.composer.noCommandsOrSkills',
      'No matching commands or skills'
    )
  }
  return translate('components.native-chat.composer.noCommands', 'No matching commands')
}

function PickerGroupHeading({ kind }: { kind: 'commands' | 'skills' }): React.JSX.Element {
  return (
    <div className="px-2 pb-1 pt-1.5 text-[11px] font-semibold uppercase tracking-[0.05em] text-muted-foreground">
      {kind === 'commands'
        ? translate('components.native-chat.composer.commands', 'Commands')
        : translate('components.native-chat.composer.skills', 'Skills')}
    </div>
  )
}

function PickerStatus({ children }: { children: React.ReactNode }): React.JSX.Element {
  return (
    <div className="flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground">
      {children}
    </div>
  )
}

function PickerOption({
  item,
  prefix,
  index,
  activeIndex,
  listboxId,
  activeItemRef,
  onChoose
}: {
  item: NativeChatPickerItem
  prefix: '/' | '$'
  index: number
  activeIndex: number
  listboxId: string
  activeItemRef: React.MutableRefObject<HTMLButtonElement | null>
  onChoose: (item: NativeChatPickerItem) => void
}): React.JSX.Element {
  const annotation = getPickerAnnotation(item)
  const selected = index === activeIndex
  return (
    <button
      id={`${listboxId}-option-${index}`}
      ref={selected ? activeItemRef : null}
      role="option"
      aria-selected={selected}
      type="button"
      onPointerDown={(event) => {
        // Why: the textarea owns query and caret state, so pointer acceptance
        // must run before the browser transfers focus to this row.
        event.preventDefault()
        onChoose(item)
      }}
      className={cn(
        'flex w-full items-start gap-2 rounded-md border border-transparent px-2 py-1.5 text-left text-[13px] hover:bg-accent hover:text-accent-foreground pointer-coarse:min-h-11',
        selected && 'border-border bg-accent text-accent-foreground'
      )}
    >
      {item.kind === 'skill' ? (
        <Package className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      ) : null}
      <span className="min-w-0 flex-1">
        <span className="block truncate font-mono font-medium">{prefix + item.name}</span>
        {item.description ? (
          <span className="block truncate text-xs text-muted-foreground">{item.description}</span>
        ) : null}
        {annotation ? (
          <span className="block truncate text-[11px] text-muted-foreground">{annotation}</span>
        ) : null}
      </span>
      {item.kind === 'skill' ? (
        <span className="shrink-0 pt-0.5 text-[11px] text-muted-foreground">
          {scopeLabel(item.sources[0]?.sourceKind)}
        </span>
      ) : null}
    </button>
  )
}

function getPickerAnnotation(item: NativeChatPickerItem): string | null {
  if (item.kind === 'command' && item.skillCollision) {
    return translate(
      'components.native-chat.composer.skillCommandCollision',
      'Also a skill name - agent decides'
    )
  }
  if (item.kind === 'skill' && item.sources.length > 1) {
    // Why: name the interpolation `sourceCount`, not `count` — a `count` option
    // makes i18next resolve plural-suffixed keys that these locales don't define.
    return translate(
      'components.native-chat.composer.skillMultipleSources',
      '{{sourceCount}} sources - agent resolves',
      { sourceCount: item.sources.length }
    )
  }
  return null
}

function scopeLabel(sourceKind: SkillSourceKind | undefined): string {
  const labels: Record<string, string> = {
    repo: translate('components.native-chat.composer.skillScopeProject', 'Project'),
    home: translate('components.native-chat.composer.skillScopePersonal', 'Personal'),
    bundled: translate('components.native-chat.composer.skillScopeBuiltIn', 'Built-in'),
    plugin: translate('components.native-chat.composer.skillScopePlugin', 'Plugin')
  }
  return sourceKind ? (labels[sourceKind] ?? '') : ''
}

export function NativeChatMentionMenu({
  query,
  files,
  loading,
  loadError,
  truncated,
  activeIndex,
  listboxId,
  onChoose
}: {
  query: string
  files: readonly NativeChatMentionFile[]
  loading: boolean
  loadError: string | null
  truncated: boolean
  activeIndex: number
  listboxId: string
  onChoose: (path: string) => void
}): React.JSX.Element {
  const activeItemRef = useRef<HTMLButtonElement | null>(null)

  useEffect(() => {
    activeItemRef.current?.scrollIntoView({ block: 'nearest' })
  }, [activeIndex, files])

  const clampedActive = Math.min(activeIndex, Math.max(0, files.length - 1))
  const emptyText =
    loadError ??
    (query.trim() === ''
      ? translate(
          'components.native-chat.composer.typeToSearchFiles',
          'Type to search workspace files'
        )
      : translate('components.native-chat.composer.noFiles', 'No matching files'))
  const moreMatchesText = translate(
    'components.native-chat.composer.moreFileMatches',
    'More matches available. Keep typing to narrow the list.'
  )
  const announcement = loading
    ? translate('components.native-chat.composer.loadingFiles', 'Loading files...')
    : files.length === 0
      ? emptyText
      : [
          translate(
            'components.native-chat.composer.fileMatchCount',
            'Matching files: {{resultCount}}',
            { resultCount: files.length }
          ),
          truncated ? moreMatchesText : null
        ]
          .filter(Boolean)
          .join('. ')
  return (
    <div
      id={listboxId}
      role="listbox"
      aria-label={translate('components.native-chat.composer.mentionFiles', 'Workspace files')}
      className="scrollbar-sleek absolute bottom-full left-0 right-0 z-20 mb-1 max-h-72 overflow-y-auto rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-[0_10px_24px_rgba(0,0,0,0.18)]"
    >
      {files.map((file, index) => {
        const FileIcon = getFileTypeIcon(file.path)
        const selected = index === clampedActive
        return (
          <button
            key={file.path}
            id={`${listboxId}-option-${index}`}
            ref={selected ? activeItemRef : null}
            role="option"
            aria-selected={selected}
            aria-label={file.path}
            type="button"
            onPointerDown={(event) => {
              // Why: the textarea owns query and caret state, so pointer acceptance
              // must run before the browser transfers focus to this row.
              event.preventDefault()
              onChoose(file.path)
            }}
            className={cn(
              'flex w-full items-center gap-2 rounded-md border border-transparent px-2 py-1.5 text-left text-[13px] hover:bg-accent hover:text-accent-foreground pointer-coarse:min-h-11',
              selected && 'border-border bg-accent text-accent-foreground'
            )}
          >
            <FileIcon
              aria-hidden="true"
              className={cn(
                'size-3.5 shrink-0 text-muted-foreground',
                getFileTypeIconColorClass(file.path)
              )}
            />
            <HighlightedMentionPath file={file} />
          </button>
        )
      })}
      {loading ? (
        <PickerStatus>
          <Loader2 className="size-3.5 animate-spin" />
          {translate('components.native-chat.composer.loadingFiles', 'Loading files...')}
        </PickerStatus>
      ) : null}
      {!loading && files.length === 0 ? <PickerStatus>{emptyText}</PickerStatus> : null}
      {!loading && files.length > 0 && truncated ? (
        <div
          aria-hidden="true"
          className="border-t border-border/60 px-2 py-1.5 text-xs text-muted-foreground"
        >
          {moreMatchesText}
        </div>
      ) : null}
      <div aria-live="polite" aria-atomic="true" className="sr-only">
        {announcement}
      </div>
    </div>
  )
}

type MentionPathSegment = {
  text: string
  directory: boolean
  matched: boolean
}

function segmentMentionPath(file: NativeChatMentionFile): MentionPathSegment[] {
  const filenameStart = Math.max(file.path.lastIndexOf('/'), file.path.lastIndexOf('\\')) + 1
  const matches = new Set(file.matchIndices)
  const segments: MentionPathSegment[] = []

  for (let index = 0; index < file.path.length; index++) {
    const directory = index < filenameStart
    const matched = matches.has(index)
    const previous = segments.at(-1)
    if (previous && previous.directory === directory && previous.matched === matched) {
      previous.text += file.path.slice(index, index + 1)
      continue
    }
    segments.push({ text: file.path.slice(index, index + 1), directory, matched })
  }

  return segments
}

function HighlightedMentionPath({ file }: { file: NativeChatMentionFile }): React.JSX.Element {
  return (
    <span aria-hidden="true" className="min-w-0 flex-1 truncate font-mono">
      {segmentMentionPath(file).map((segment, index) => (
        <span
          key={`${index}:${segment.text}`}
          data-native-chat-mention-match={segment.matched || undefined}
          className={cn(
            segment.directory ? 'text-muted-foreground' : 'text-foreground',
            segment.matched &&
              'rounded-[2px] bg-ring/10 font-semibold text-foreground underline decoration-ring/60 underline-offset-2'
          )}
        >
          {segment.text}
        </span>
      ))}
    </span>
  )
}
