import { describe, expect, it } from 'vitest'

import {
  AUTOMATION_PUSH_GUARD_SENTINEL,
  buildAutomationRunPrompt
} from './automation-prompt-guard'

describe('buildAutomationRunPrompt', () => {
  it('prepends the push guard when autoPushEnabled is not true', () => {
    const result = buildAutomationRunPrompt({ prompt: 'Fix the flaky checkout test', autoPushEnabled: false })

    expect(result.startsWith(AUTOMATION_PUSH_GUARD_SENTINEL)).toBe(true)
    expect(result).toContain('Do NOT run `git push`')
    expect(result.endsWith('Fix the flaky checkout test')).toBe(true)
  })

  it('treats undefined as guarded (default-deny)', () => {
    const result = buildAutomationRunPrompt({ prompt: 'Tidy the changelog' })

    expect(result).toContain(AUTOMATION_PUSH_GUARD_SENTINEL)
  })

  it('passes the prompt through untouched when autoPush is explicitly enabled', () => {
    const result = buildAutomationRunPrompt({ prompt: 'Ship the release branch', autoPushEnabled: true })

    expect(result).toBe('Ship the release branch')
  })
})
