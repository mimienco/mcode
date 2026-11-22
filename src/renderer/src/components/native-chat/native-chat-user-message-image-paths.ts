export type UserMessageImageExtraction = {
  text: string
  imagePaths: string[]
}
// Why: quoted forms may contain spaces but must contain a path separator so
// plain filenames like "screenshot.png" stay prose; the bare form stops at
// whitespace, so drive-letter/UNC/POSIX prefixes are required instead.
const IMAGE_EXT = String.raw`\.(?:png|jpe?g|jfif|gif|webp|bmp|avif)`
const IMAGE_PATH_TOKEN = new RegExp(
  `@?"([^"\\n]*[/\\\\][^"\\n]+?${IMAGE_EXT})"` +
    `|@?'([^'\\n]*[/\\\\][^'\\n]+?${IMAGE_EXT})'` +
    // Bare absolute paths can contain spaces (for example C:\\Users\\Milan M\\...).
    // The image extension is the terminator, so accepting spaces cannot consume
    // following prose beyond the path and keeps relative prose/file names excluded.
    `|@?((?:(?:[A-Za-z]:)?[/\\\\][^"'\\r\\n]+?${IMAGE_EXT}))`,
  'gi'
)

/** Splits pasted image-path tokens out of a user message so they render as
 *  preview chips instead of raw text; paths come back order-preserving, deduped. */
export function extractUserMessageImagePaths(text: string): UserMessageImageExtraction {
  const imagePaths: string[] = []
  const stripped = text.replace(IMAGE_PATH_TOKEN, (_match, double, single, bare) => {
    const path = double ?? single ?? bare
    if (path && !imagePaths.includes(path)) {
      imagePaths.push(path)
    }
    return ''
  })
  return { text: stripped.trim(), imagePaths }
}
