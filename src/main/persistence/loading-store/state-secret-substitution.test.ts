import { createHash } from 'node:crypto'
import { describe, expect, it } from 'vitest'
import { substituteSecretSentinels, type SecretSubstitution } from './state-secret-substitution'

// PERF-08 commit 2a: the single-pass sentinel substitution must be
// byte-identical to the legacy two-replacements-per-secret loop across
// `$`-pattern torture cases, adjacency, unicode, and degraded mode.

/** Legacy oracle: exactly what buildStateToSave did before PERF-08. */
function legacySubstitute(
  serialized: string,
  secretSubs: readonly SecretSubstitution[],
  protectedStorageDegraded: boolean
): { payload: string; stateHash: string } {
  let payload = serialized
  let hashInput = serialized
  for (const { sentinel, blob, hashValue } of secretSubs) {
    const escapedSentinel = JSON.stringify(sentinel).slice(1, -1)
    payload = payload.replace(escapedSentinel, () => JSON.stringify(blob).slice(1, -1))
    hashInput = hashInput.replace(escapedSentinel, () => JSON.stringify(hashValue).slice(1, -1))
  }
  const stateHash = createHash('sha1')
    .update(protectedStorageDegraded ? 'safeStorage-degraded\0' : '')
    .update(hashInput)
    .digest('hex')
  return { payload, stateHash }
}

const DOLLAR_TORTURE = ['$&', "$'$", '$`', '$1$2', '$$', 'leading $ middle $ trailing']

function randomUUIDLike(seed: number): string {
  return `mcode-secret-slot-${seed.toString(16).padStart(12, '0')}-${(seed * 7919)
    .toString(16)
    .padStart(8, '0')}`
}

describe('secret sentinel substitution', () => {
  it('matches the legacy loop for plain secrets', () => {
    const subs: SecretSubstitution[] = [
      { sentinel: randomUUIDLike(1), blob: 'enc-a', hashValue: 'plain-a' },
      { sentinel: randomUUIDLike(2), blob: 'enc-b', hashValue: 'plain-b' }
    ]
    const serialized = `{"x":1,"settings":{"cookie":"${subs[0].sentinel}"},"ui":{"k":"${subs[1].sentinel}"}}`
    const singlePass = substituteSecretSentinels({
      serialized,
      secretSubs: subs,
      protectedStorageDegraded: false
    })
    const legacy = legacySubstitute(serialized, subs, false)
    expect(singlePass.payload).toBe(legacy.payload)
    expect(singlePass.stateHash).toBe(legacy.stateHash)
  })

  it('is byte-identical under `$`-torture in blobs and guard values', () => {
    for (const [index, pattern] of DOLLAR_TORTURE.entries()) {
      const subs: SecretSubstitution[] = [
        {
          sentinel: randomUUIDLike(index + 10),
          blob: `enc-${pattern}`,
          hashValue: `guard-${pattern}`
        }
      ]
      const serialized = `{"settings":{"cookie":"${subs[0].sentinel}"}}`
      const singlePass = substituteSecretSentinels({
        serialized,
        secretSubs: subs,
        protectedStorageDegraded: false
      })
      const legacy = legacySubstitute(serialized, subs, false)
      expect(singlePass.payload).toBe(legacy.payload)
      expect(singlePass.stateHash).toBe(legacy.stateHash)
    }
  })

  it('handles adjacent sentinels and start/end positions', () => {
    const s1 = randomUUIDLike(21)
    const s2 = randomUUIDLike(22)
    const subs: SecretSubstitution[] = [
      { sentinel: s1, blob: 'B1', hashValue: 'H1' },
      { sentinel: s2, blob: 'B2', hashValue: 'H2' }
    ]
    // Adjacent sentinels with no separator; first at position 0, last at EOF.
    const serialized = `${s1}${s2}`
    const singlePass = substituteSecretSentinels({
      serialized,
      secretSubs: subs,
      protectedStorageDegraded: false
    })
    const legacy = legacySubstitute(serialized, subs, false)
    expect(singlePass.payload).toBe(legacy.payload)
    expect(singlePass.stateHash).toBe(legacy.stateHash)
  })

  it('prepends the degraded marker to the hash preimage only', () => {
    const subs: SecretSubstitution[] = [
      { sentinel: randomUUIDLike(31), blob: 'enc', hashValue: 'guard' }
    ]
    const serialized = `{"cookie":"${subs[0].sentinel}"}`
    const healthy = substituteSecretSentinels({
      serialized,
      secretSubs: subs,
      protectedStorageDegraded: false
    })
    const degraded = substituteSecretSentinels({
      serialized,
      secretSubs: subs,
      protectedStorageDegraded: true
    })

    expect(degraded.payload).toBe(healthy.payload)
    expect(degraded.stateHash).not.toBe(healthy.stateHash)

    const expectedDegraded = createHash('sha1')
      .update('safeStorage-degraded\0')
      .update(`{"cookie":"guard"}`)
      .digest('hex')
    expect(degraded.stateHash).toBe(expectedDegraded)
  })

  it('throws instead of silently skipping a missing sentinel', () => {
    const subs: SecretSubstitution[] = [
      { sentinel: randomUUIDLike(41), blob: 'enc', hashValue: 'g' }
    ]
    expect(() =>
      substituteSecretSentinels({
        serialized: '{"other":1}',
        secretSubs: subs,
        protectedStorageDegraded: false
      })
    ).toThrow(/secret sentinel not found/)
  })

  it('survives randomized multi-secret sequences (differential)', () => {
    let seed = 12345
    const rand = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648
    for (let round = 0; round < 50; round += 1) {
      const count = 1 + Math.floor(rand() * 6)
      const subs: SecretSubstitution[] = []
      let filler = ''
      for (let index = 0; index < count; index += 1) {
        const sentinel = randomUUIDLike(round * 100 + index)
        const blob = ['enc', '$&', 'unicode-äöü✓', ''][index % 4] + index
        const hashValue = ['g', "$'", 'plain', 'x-y'][index % 4]
        subs.push({ sentinel, blob, hashValue })
        filler += `{"k${index}":"${sentinel}"},"pre-${rand().toFixed(3)}":${index % 7},`
      }
      const serialized = `{${filler.slice(0, -1)}}`
      const singlePass = substituteSecretSentinels({
        serialized,
        secretSubs: subs,
        protectedStorageDegraded: rand() > 0.5
      })
      const degraded = singlePass
      void degraded
      const legacyProtected = round % 2 === 0
      const legacy = legacySubstitute(serialized, subs, legacyProtected)
      const passAgain = substituteSecretSentinels({
        serialized,
        secretSubs: subs,
        protectedStorageDegraded: legacyProtected
      })
      expect(passAgain.payload).toBe(legacy.payload)
      expect(passAgain.stateHash).toBe(legacy.stateHash)
    }
  })
})
