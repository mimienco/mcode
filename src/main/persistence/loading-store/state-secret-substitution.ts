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

export type SerializedSplit = {
  /** segments.length === secretSubs.length + 1 */
  segments: string[]
  /** Source index of each sentinel occurrence, in submission order. */
  starts: number[]
}

/** Single forward scan locating every escaped sentinel. No secret material
 *  crosses this boundary — safe to run inside a worker_threads entry. */
export function splitSerializedAtSentinels(
  serialized: string,
  escapedSentinels: readonly string[]
): SerializedSplit {
  const segments: string[] = []
  const starts: number[] = []
  let cursor = 0
  let searchFrom = 0
  for (const escapedSentinel of escapedSentinels) {
    const index = serialized.indexOf(escapedSentinel, searchFrom)
    if (index === -1) {
      throw new Error(
        `[persistence] secret sentinel not found during substitution (${escapedSentinel.slice(0, 24)}…)!`
      )
    }
    segments.push(serialized.slice(cursor, index))
    starts.push(index)
    cursor = index + escapedSentinel.length
    searchFrom = index + escapedSentinel.length
  }
  segments.push(serialized.slice(cursor))
  return { segments, starts }
}

export function substituteSecretSentinels({
  serialized,
  secretSubs,
  protectedStorageDegraded
}: SubstituteSecretSentinelsArgs): SubstitutedState {
  const escaped = secretSubs.map((sub) => escapeForEmbedding(sub.sentinel))
  const { segments } = splitSerializedAtSentinels(serialized, escaped)

  // Why: assemble both outputs from the shared split — payload embeds the
  // ciphertext blobs, hash input the normalized guard values; function-form
  // embedding keeps `$` inert exactly like the legacy replace loop.
  const payloadBuilder: string[] = []
  const hashBuilder: string[] = []
  for (let index = 0; index < segments.length; index += 1) {
    payloadBuilder.push(segments[index])
    hashBuilder.push(segments[index])
    const sub = secretSubs[index]
    if (sub === undefined) {
      continue
    }
    payloadBuilder.push(JSON.stringify(sub.blob).slice(1, -1))
    hashBuilder.push(JSON.stringify(sub.hashValue).slice(1, -1))
  }
  const stateHash = createHash('sha1')
    .update(protectedStorageDegraded ? 'safeStorage-degraded\0' : '')
    .update(hashBuilder.join(''))
    .digest('hex')

  return { payload: payloadBuilder.join(''), stateHash }
}
