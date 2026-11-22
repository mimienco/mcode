import type { SavedSnippet } from './saved-snippet-types'

export const MAX_SAVED_SNIPPETS = 100
export const MAX_SAVED_SNIPPET_NAME_LENGTH = 80
export const MAX_SAVED_SNIPPET_TEXT_LENGTH = 16_384

export function normalizeSavedSnippets(value: unknown): SavedSnippet[] {
  if (!Array.isArray(value)) {
    return []
  }
  const normalized: SavedSnippet[] = []
  const ids = new Set<string>()
  for (const candidate of value) {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
      continue
    }
    const record = candidate as Record<string, unknown>
    if (
      typeof record.id !== 'string' ||
      typeof record.name !== 'string' ||
      typeof record.text !== 'string'
    ) {
      continue
    }
    const id = record.id.trim()
    const name = record.name.trim().slice(0, MAX_SAVED_SNIPPET_NAME_LENGTH)
    const text = record.text.trimEnd().slice(0, MAX_SAVED_SNIPPET_TEXT_LENGTH)
    if (!id || !name || !text.trim() || ids.has(id)) {
      continue
    }
    ids.add(id)
    normalized.push({ id, name, text })
    if (normalized.length === MAX_SAVED_SNIPPETS) {
      break
    }
  }
  return normalized
}
