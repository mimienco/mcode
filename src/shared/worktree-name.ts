import { replaceKnownEmojiWithShortcodes } from './emoji-shortcode-catalog'

const WINDOWS_RESERVED_DEVICE_NAME = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$/i

function containsEmoji(input: string): boolean {
  return /[\p{Emoji_Presentation}\p{Extended_Pictographic}\p{Regional_Indicator}\u20e3]/u.test(
    input
  )
}

/**
 * Normalize a user-facing workspace name into the leaf used by both its Git
 * branch and filesystem directory. Keep this shared so previews never promise
 * a name that the main-process create boundary silently changes later.
 */
export function sanitizeWorktreeName(input: string): string {
  let sanitized = replaceKnownEmojiWithShortcodes(input)
    .trim()
    .replace(/[^\p{L}\p{N}._-]+/gu, '-')
    .replace(/-+/g, '-')
    .replace(/\.{2,}/g, '.')
    .replace(/^[.-]+|[.-]+$/g, '')

  if (!sanitized && containsEmoji(input)) {
    sanitized = 'workspace'
  }

  if (!sanitized || sanitized === '.' || sanitized === '..') {
    throw new Error('Invalid worktree name')
  }

  // Git rejects a ref component ending in `.lock`; Windows reserves device
  // basenames even when an extension follows. Normalize both before either
  // platform gets a chance to fail with a low-level path/ref error.
  if (/\.lock$/i.test(sanitized)) {
    sanitized = `${sanitized.slice(0, -5)}-lock`
  }
  if (WINDOWS_RESERVED_DEVICE_NAME.test(sanitized)) {
    sanitized = `workspace-${sanitized}`
  }

  return sanitized
}
