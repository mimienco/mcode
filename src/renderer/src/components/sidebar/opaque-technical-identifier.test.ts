import { describe, expect, it } from 'vitest'

import {
  humanizeOpaqueSidebarLabel,
  isOpaqueTechnicalIdentifier
} from './opaque-technical-identifier'

describe('isOpaqueTechnicalIdentifier', () => {
  it('recognizes UUID labels and workspace-key UUIDs', () => {
    expect(isOpaqueTechnicalIdentifier('9689e8d2-5dd1-4c0f-a8a6-b8224a242f10')).toBe(true)
    expect(isOpaqueTechnicalIdentifier('folder:9689e8d2-5dd1-4c0f-a8a6-b8224a242f10')).toBe(true)
  })

  it('recognizes abbreviated and full hexadecimal hashes', () => {
    expect(isOpaqueTechnicalIdentifier('a3f91c72')).toBe(true)
    expect(isOpaqueTechnicalIdentifier('a3f91c7208ef4da249f8a3a53332371afbf65b1c')).toBe(true)
    expect(isOpaqueTechnicalIdentifier('sha:a3f91c72')).toBe(true)
  })

  it('keeps human-authored labels visible', () => {
    expect(isOpaqueTechnicalIdentifier('Fix workspace navigation')).toBe(false)
    expect(isOpaqueTechnicalIdentifier('release-2026-08-23')).toBe(false)
    expect(isOpaqueTechnicalIdentifier('deadbeef')).toBe(false)
    expect(isOpaqueTechnicalIdentifier('abc1234')).toBe(false)
  })

  it('replaces an opaque primary label with a human fallback', () => {
    expect(humanizeOpaqueSidebarLabel('9689e8d2-5dd1-4c0f-a8a6-b8224a242f10', 'Codex')).toBe(
      'Codex'
    )
    expect(humanizeOpaqueSidebarLabel('Review navigation', 'Codex')).toBe('Review navigation')
  })
})
