import { File, FileCog, FileLock, FileTerminal, Smartphone, type LucideIcon } from 'lucide-react'
import { COMPOUND_EXTENSIONS, FILE_ICON_BY_EXTENSION } from './file-type-icon-extension-table'
import { FILE_ICON_BY_NAME } from './file-type-icon-name-table'

function getFilename(filePath: string | undefined | null): string {
  if (!filePath) {
    return ''
  }
  const lastSlash = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'))
  return lastSlash >= 0 ? filePath.slice(lastSlash + 1) : filePath
}

function getExtension(filename: string): string {
  const lowerName = filename.toLowerCase()
  const compoundExtension = COMPOUND_EXTENSIONS.find((ext) => lowerName.endsWith(`.${ext}`))
  if (compoundExtension) {
    return compoundExtension
  }

  const lastDot = filename.lastIndexOf('.')
  if (lastDot <= 0 || lastDot === filename.length - 1) {
    return ''
  }

  return filename.slice(lastDot + 1).toLowerCase()
}

export function getFileTypeIcon(filePath: string | undefined | null): LucideIcon {
  const filename = getFilename(filePath)
  if (!filename) {
    return File
  }
  const lowerName = filename.toLowerCase()
  const exactMatch = FILE_ICON_BY_NAME[lowerName]
  if (exactMatch) {
    return exactMatch
  }

  // Why: simulator tabs reuse EditorFileTab chrome with a synthetic label path.
  if (lowerName === 'mobile emulator' || lowerName === 'simulator') {
    return Smartphone
  }

  if (lowerName === '.env' || lowerName.startsWith('.env.')) {
    return FileLock
  }

  if (lowerName === 'dockerfile' || lowerName.startsWith('dockerfile.')) {
    return FileCog
  }

  if (lowerName === 'makefile' || lowerName.startsWith('makefile.')) {
    return FileTerminal
  }

  // Why: filename/extension matching keeps icons deterministic for SSH worktrees
  // where OS-native file associations are not available.
  return FILE_ICON_BY_EXTENSION[getExtension(filename)] ?? File
}

// Why: a small category bucket (not per-extension) keeps this table maintainable;
// anything unmapped falls back to the menu's muted foreground.
const FILE_ICON_COLOR_GROUPS: readonly (readonly [string, readonly string[]])[] = [
  ['text-file-icon-typescript', ['ts', 'tsx', 'mts', 'cts', 'd.ts']],
  ['text-file-icon-javascript', ['js', 'jsx', 'mjs', 'cjs']],
  ['text-file-icon-config', ['json', 'jsonc', 'yaml', 'yml', 'toml', 'lock']],
  ['text-file-icon-docs', ['md', 'mdx', 'txt', 'rst', 'adoc']],
  ['text-file-icon-styles', ['css', 'scss', 'sass', 'less', 'html', 'htm', 'vue', 'svelte']],
  ['text-file-icon-dynamic', ['py', 'rb']],
  ['text-file-icon-rust', ['rs']],
  ['text-file-icon-go', ['go']],
  ['text-file-icon-media', ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'ico', 'avif', 'bmp']],
  ['text-file-icon-shell', ['sh', 'bash', 'zsh', 'fish', 'ps1', 'bat', 'cmd']],
  ['text-file-icon-secret', ['env']]
]

const FILE_ICON_COLOR_BY_EXTENSION: Record<string, string> = Object.fromEntries(
  FILE_ICON_COLOR_GROUPS.flatMap(([color, extensions]) =>
    extensions.map((extension) => [extension, color] as const)
  )
)

/** Tailwind text-color class matching {@link getFileTypeIcon} for the same path. */
export function getFileTypeIconColorClass(filePath: string | undefined | null): string {
  const filename = getFilename(filePath)
  if (!filename) {
    return ''
  }
  const lowerName = filename.toLowerCase()
  if (lowerName === '.env' || lowerName.startsWith('.env.')) {
    return FILE_ICON_COLOR_BY_EXTENSION['env']
  }
  return FILE_ICON_COLOR_BY_EXTENSION[getExtension(filename)] ?? ''
}
