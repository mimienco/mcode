const UUID_LABEL =
  /^(?:[a-z]+:)?[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
const HEX_DIGEST_LABEL =
  /^(?:[a-z]+:)?(?=[0-9a-f]{8}(?:[0-9a-f]{4}){0,2}$|[0-9a-f]{20}$|[0-9a-f]{32}$|[0-9a-f]{40}$|[0-9a-f]{64}$)(?=.*[a-f])(?=.*\d)[0-9a-f]+$/i

/**
 * Generated IDs are useful for lookup and diagnostics, but make poor primary
 * labels in the navigation hierarchy.
 */
export function isOpaqueTechnicalIdentifier(value: string): boolean {
  const label = value.trim()
  return UUID_LABEL.test(label) || HEX_DIGEST_LABEL.test(label)
}

export function humanizeOpaqueSidebarLabel(value: string, fallback: string): string {
  return isOpaqueTechnicalIdentifier(value) ? fallback : value
}
