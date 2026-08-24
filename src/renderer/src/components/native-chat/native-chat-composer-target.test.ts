import { describe, expect, it } from 'vitest'
import { buildNativeChatPrompt } from './native-chat-composer-target'

describe('buildNativeChatPrompt', () => {
  it('serializes selected files before the typed message', () => {
    expect(buildNativeChatPrompt('review these', ['src/App.tsx', 'docs/My Guide.md'])).toBe(
      '@src/App.tsx @"docs/My Guide.md" review these'
    )
  })

  it('supports a file-only prompt without trailing whitespace', () => {
    expect(buildNativeChatPrompt('', ['src/App.tsx'])).toBe('@src/App.tsx')
  })
})
