import { createHash } from 'node:crypto'

// Why: PERF-08 — buildStateToSave previously ran two full-payload string
// replacements PER SECRET (O(secrets x MB)) plus a separate hash-input copy.
// This module performs the same substitution in a SINGLE forward pass over
// the serialized state: locate every sentinel once, then assemble payload and
// hash input side by side. Semantics are locked to the legacy
// String#replace(first-occurrence, function-form) behavior by the
// differential test (including `$`-patterns in blobs/values).

export type SecretSubstitution = {
  sentinel: string
  blob: string
  hashValue: string
}

export type SubstituteSecretSentinelsArgs = {
  /** Full JSON.stringify(stateToSave) with sentinels in place. */
  serialized: string
  secretSubs: readonly SecretSubstitution[]
  /** Why: prepended to the hash preimage so a degraded-safeStorage write can
   *  be superseded by a healthy retry even when plaintext state is unchanged. */
  protectedStorageDegraded: boolean
}

export type SubstitutedState = {
  payload: string
  stateHash: string
}

function escapeForEmbedding(value: string): string {
  return JSON.stringify(value).slice(1, -1)
}

export function substituteSecretSentinels({
  serialized,
  secretSubs,
  protectedStorageDegraded
}: SubstituteSecretSentinelsArgs): SubstitutedState {
  // Why: each sentinel is a fresh UUID minted immediately before serialization,
  // so it occurs exactly once; the walk stays strictly forward per secret.
  const located: {
    escapedSentinelLength: number
    index: number
    blobEmbedded: string
    hashValueEmbedded: string
  }[] = []
  let searchFrom = 0
  for (const sub of secretSubs) {
    const escapedSentinel = escapeForEmbedding(sub.sentinel)
    const index = serialized.indexOf(escapedSentinel, searchFrom)
    if (index === -1) {
      throw new Error(
        `[persistence] secret sentinel not found during substitution (${sub.sentinel.slice(0, 24)}…)`
      )
    }
    located.push({
      escapedSentinelLength: escapedSentinel.length,
      index,
      blobEmbedded: JSON.stringify(sub.blob).slice(1, -1),
      hashValueEmbedded: JSON.stringify(sub.hashValue).slice(1, -1)
    })
    searchFrom = index + escapedSentinel.length
  }

  // Why: build both outputs in one walk — every sentinel position splits the
  // string once for the payload (ciphertext) and once for the hash input
  // (normalized guard value). Function-form embedding keeps `$` inert.
  const payloadBuilder: string[] = []
  const hashBuilder: string[] = []
  let writeCursor = 0
  for (const hit of located) {
    const segmentEnd = hit.index
    payloadBuilder.push(serialized.slice(writeCursor, segmentEnd))
    hashBuilder.push(serialized.slice(writeCursor, segmentEnd))
    writeCursor = segmentEnd + hit.escapedSentinelLength
    payloadBuilder.push(hit.blobEmbedded)
    hashBuilder.push(hit.hashValueEmbedded)
  }
  payloadBuilder.push(serialized.slice(writeCursor))
  hashBuilder.push(serialized.slice(writeCursor))

  const stateHash = createHash('sha1')
    .update(protectedStorageDegraded ? 'safeStorage-degraded\0' : '')
    .update(hashBuilder.join(''))
    .digest('hex')

  return { payload: payloadBuilder.join(''), stateHash }
}
