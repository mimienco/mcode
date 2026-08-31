/**
 * Canonical external URLs for user-facing surfaces: documentation, changelog and
 * updater feeds, feedback API, telemetry disclosure.
 *
 * TODO(rebrand): temporary origin — the product docs currently live on the
 * upstream Orca site. Replace DOCS_ORIGIN with the MCode docs host once
 * docs/user/ is published, and repoint the updater feed and feedback API at
 * MCode-owned endpoints. Every consumer below must import from here so the
 * swap is a one-line change.
 */
export const DOCS_ORIGIN = 'https://www.onorca.dev'

export const DOCS_URL = `${DOCS_ORIGIN}/docs`
export const CHANGELOG_URL = `${DOCS_ORIGIN}/changelog`
export const CHANGELOG_FEED_URL = `${DOCS_ORIGIN}/whats-new/changelog.json`
export const NUDGE_FEED_URL = `${DOCS_ORIGIN}/whats-new/nudge.json`
export const PRIVACY_URL = `${DOCS_ORIGIN}/privacy`
export const FEEDBACK_API_URL = `${DOCS_ORIGIN}/v1/feedback`

/** Build a docs deep link from a path under /docs, e.g. docsPageUrl('model/worktrees'). */
export function docsPageUrl(path: string): string {
  const trimmed = path.replace(/^\/+/, '')
  return trimmed.startsWith('docs/') ? `${DOCS_ORIGIN}/${trimmed}` : `${DOCS_URL}/${trimmed}`
}
