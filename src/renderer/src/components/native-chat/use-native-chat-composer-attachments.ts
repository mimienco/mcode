import { useCallback, useRef, useState, type RefObject } from 'react'
import { translate } from '@/i18n/i18n'
import { isNativeChatImageAttachmentPath } from './native-chat-image-paste'
import {
  nativeChatComposerTargetIsRemote,
  type NativeChatResolvedTarget
} from './native-chat-composer-target'
import type { NativeChatComposerImageAttachment } from './NativeChatComposerField'
import { setBoundedScopeCacheEntry } from './native-chat-composer-scope-cache'

export type NativeChatComposerFileReference = {
  id: string
  path: string
}

export type UseNativeChatComposerAttachmentsArgs = {
  attachmentScopeKey: string
  resolveTarget: () => NativeChatResolvedTarget | null
  textareaRef: RefObject<HTMLTextAreaElement | null>
  setNotice: (notice: string | null) => void
}

export function useNativeChatComposerAttachments({
  attachmentScopeKey,
  resolveTarget,
  textareaRef,
  setNotice
}: UseNativeChatComposerAttachmentsArgs): {
  imageAttachments: NativeChatComposerImageAttachment[]
  fileReferences: NativeChatComposerFileReference[]
  appendImageAttachments: (paths: string[]) => void
  appendFileReferences: (paths: string[]) => void
  attachResolvedPaths: (paths: string[]) => void
  clearImageAttachments: () => void
  clearFileReferences: () => void
  removeImageAttachment: (id: string) => void
  removeFileReference: (id: string) => void
} {
  const [imageAttachments, setImageAttachments] = useState<NativeChatComposerImageAttachment[]>(
    () => readNativeChatAttachmentCache(attachmentScopeKey)
  )
  const [fileReferences, setFileReferences] = useState<NativeChatComposerFileReference[]>(() =>
    readNativeChatFileReferenceCache(attachmentScopeKey)
  )
  const attachmentCounter = useRef(0)

  // Why: one composer instance can switch panes; pending chips belong to the
  // stable pane scope and must never follow the component into another pane.
  const lastScopeKey = useRef(attachmentScopeKey)
  if (lastScopeKey.current !== attachmentScopeKey) {
    lastScopeKey.current = attachmentScopeKey
    setImageAttachments(readNativeChatAttachmentCache(attachmentScopeKey))
    setFileReferences(readNativeChatFileReferenceCache(attachmentScopeKey))
  }

  const updateImageAttachments = useCallback(
    (
      updater: (
        previous: NativeChatComposerImageAttachment[]
      ) => NativeChatComposerImageAttachment[]
    ) => {
      setImageAttachments((previous) => {
        const next = updater(previous)
        writeNativeChatAttachmentCache(attachmentScopeKey, next)
        return next
      })
    },
    [attachmentScopeKey]
  )

  const updateFileReferences = useCallback(
    (
      updater: (previous: NativeChatComposerFileReference[]) => NativeChatComposerFileReference[]
    ) => {
      setFileReferences((previous) => {
        const next = updater(previous)
        writeNativeChatFileReferenceCache(attachmentScopeKey, next)
        return next
      })
    },
    [attachmentScopeKey]
  )

  const nextAttachmentId = useCallback(() => {
    attachmentCounter.current += 1
    return `${Date.now()}-${attachmentCounter.current}`
  }, [])

  const appendImageAttachments = useCallback(
    (paths: string[]) => {
      if (paths.length === 0) {
        return
      }
      updateImageAttachments((previous) => [
        ...previous,
        ...paths.map((path) => ({ id: nextAttachmentId(), path }))
      ])
    },
    [nextAttachmentId, updateImageAttachments]
  )

  const appendFileReferences = useCallback(
    (paths: string[]) => {
      if (paths.length === 0) {
        return
      }
      updateFileReferences((previous) => {
        const existing = new Set(previous.map((reference) => reference.path))
        const added: NativeChatComposerFileReference[] = []
        for (const path of paths) {
          if (path.length === 0 || existing.has(path)) {
            continue
          }
          existing.add(path)
          added.push({ id: nextAttachmentId(), path })
        }
        return added.length > 0 ? [...previous, ...added] : previous
      })
      setNotice(null)
      requestAnimationFrame(() => textareaRef.current?.focus())
    },
    [nextAttachmentId, setNotice, textareaRef, updateFileReferences]
  )

  // Attach paths the TARGET AGENT can read: local paths for local worktrees,
  // already-uploaded remote paths for SSH worktrees. Images and ordinary files
  // remain removable chips until submit, so the visual state matches the payload.
  const attachResolvedPaths = useCallback(
    (paths: string[]) => {
      const target = resolveTarget()
      if (!target || nativeChatComposerTargetIsRemote(target.ptyId)) {
        setNotice(
          translate(
            'components.native-chat.composer.localAttachmentUnsupported',
            'Local attachments are not available for remote sessions.'
          )
        )
        return
      }
      appendImageAttachments(paths.filter(isNativeChatImageAttachmentPath))
      appendFileReferences(paths.filter((path) => !isNativeChatImageAttachmentPath(path)))
      setNotice(null)
      requestAnimationFrame(() => textareaRef.current?.focus())
    },
    [appendFileReferences, appendImageAttachments, resolveTarget, setNotice, textareaRef]
  )

  return {
    imageAttachments,
    fileReferences,
    appendImageAttachments,
    appendFileReferences,
    attachResolvedPaths,
    clearImageAttachments: () => updateImageAttachments(() => []),
    clearFileReferences: () => updateFileReferences(() => []),
    removeImageAttachment: (id) =>
      updateImageAttachments((previous) => previous.filter((attachment) => attachment.id !== id)),
    removeFileReference: (id) =>
      updateFileReferences((previous) => previous.filter((reference) => reference.id !== id))
  }
}

const attachmentCache = new Map<string, NativeChatComposerImageAttachment[]>()
const fileReferenceCache = new Map<string, NativeChatComposerFileReference[]>()

export function readNativeChatAttachmentCache(
  scopeKey: string
): NativeChatComposerImageAttachment[] {
  return [...(attachmentCache.get(scopeKey) ?? [])]
}

export function readNativeChatFileReferenceCache(
  scopeKey: string
): NativeChatComposerFileReference[] {
  return [...(fileReferenceCache.get(scopeKey) ?? [])]
}

function writeNativeChatAttachmentCache(
  scopeKey: string,
  attachments: readonly NativeChatComposerImageAttachment[]
): void {
  writeScopedAttachmentCache(attachmentCache, scopeKey, attachments)
}

function writeNativeChatFileReferenceCache(
  scopeKey: string,
  references: readonly NativeChatComposerFileReference[]
): void {
  writeScopedAttachmentCache(fileReferenceCache, scopeKey, references)
}

function writeScopedAttachmentCache<T>(
  cache: Map<string, T[]>,
  scopeKey: string,
  entries: readonly T[]
): void {
  if (entries.length === 0) {
    cache.delete(scopeKey)
    return
  }
  setBoundedScopeCacheEntry(cache, scopeKey, [...entries])
}

export function clearNativeChatAttachmentCacheForTests(): void {
  attachmentCache.clear()
  fileReferenceCache.clear()
}
