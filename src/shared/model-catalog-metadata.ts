export type ModelCatalogPricing = {
  /** USD per one million input tokens, as published by the provider catalog. */
  inputPerMillion?: number
  /** USD per one million output tokens, as published by the provider catalog. */
  outputPerMillion?: number
}

/** Optional discovery metadata used by rich model pickers. Absence means the
 * discovery source does not publish the field; it must not be treated as zero. */
export type ModelCatalogMetadata = {
  providerId?: string
  providerLabel?: string
  contextWindowTokens?: number
  maxOutputTokens?: number
  pricing?: ModelCatalogPricing
  /** `available` means the authoritative host probe returned this model.
   * `unavailable` is reserved for a previously selected row absent from it. */
  availability?: 'available' | 'unavailable'
}
