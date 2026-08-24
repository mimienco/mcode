import { describe, expect, it } from 'vitest'
import {
  applyAgentRunProfile,
  getAgentRunProfileCapabilities,
  resolveTuiAgentRunProfile
} from './agent-run-profile'

describe('agent run profile', () => {
  it('maps the provider-neutral presets without widening Cursor auto-edits to bypass', () => {
    const result = applyAgentRunProfile({
      permission: 'auto-edits',
      interactionMode: 'build',
      agentDefaultArgs: {
        claude: '--dangerously-skip-permissions',
        codex: '--dangerously-bypass-approvals-and-sandbox',
        gemini: '--yolo',
        cursor: '--yolo',
        grok: '--permission-mode bypassPermissions'
      }
    })

    expect(result.agentDefaultArgs).toMatchObject({
      claude: '--permission-mode acceptEdits',
      codex: '--sandbox workspace-write --ask-for-approval on-request',
      gemini: '--approval-mode auto_edit',
      cursor: '',
      grok: '--permission-mode acceptEdits'
    })
    expect(getAgentRunProfileCapabilities('cursor').permissions['auto-edits'].kind).toBe(
      'unsupported'
    )
  })

  it('keeps Build permission intent while providers with a combined enum enter Plan', () => {
    const planned = applyAgentRunProfile({
      permission: 'auto-edits',
      interactionMode: 'plan',
      agentDefaultArgs: {}
    })

    expect(planned.agentDefaultArgs).toMatchObject({
      claude: '--permission-mode plan',
      gemini: '--approval-mode plan',
      cursor: '--mode plan',
      grok: '--permission-mode acceptEdits'
    })
    const building = applyAgentRunProfile({
      permission: 'auto-edits',
      interactionMode: 'build',
      agentDefaultArgs: planned.agentDefaultArgs
    })
    expect(building.agentDefaultArgs).toMatchObject({
      claude: '--permission-mode acceptEdits',
      gemini: '--approval-mode auto_edit',
      cursor: '',
      grok: '--permission-mode acceptEdits'
    })
    expect(resolveTuiAgentRunProfile('claude', '--permission-mode plan')).toEqual({
      permission: undefined,
      interactionMode: 'plan',
      mcodeOwned: true
    })
  })

  it('uses the provider-documented Claude Ask mode', () => {
    const result = applyAgentRunProfile({
      permission: 'ask',
      interactionMode: 'build',
      agentDefaultArgs: { claude: '--dangerously-skip-permissions' }
    })

    expect(result.agentDefaultArgs.claude).toBe('--permission-mode default')
  })

  it('preserves free-form overrides instead of rewriting unknown arguments', () => {
    const result = applyAgentRunProfile({
      permission: 'ask',
      interactionMode: 'build',
      agentDefaultArgs: { codex: '--model gpt-5.6-sol' }
    })

    expect(result.agentDefaultArgs.codex).toBe('--model gpt-5.6-sol')
    expect(resolveTuiAgentRunProfile('codex', '--model gpt-5.6-sol').mcodeOwned).toBe(false)
  })

  it('does not confuse an absent provider Plan mapping with empty Build arguments', () => {
    expect(resolveTuiAgentRunProfile('codex', '')).toEqual({
      permission: 'ask',
      interactionMode: 'build',
      mcodeOwned: true
    })
  })
})
