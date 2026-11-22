import { describe, expect, it } from 'vitest'
import type { CodexLiveSessionCatalog } from './codex-live-session-catalog'
import { prepareCodexLiveSessionUpdate } from './codex-live-session-catalog'

const catalog: CodexLiveSessionCatalog = {
  models: [
    {
      id: 'gpt-5.6-sol',
      label: 'GPT-5.6 Sol',
      isDefault: true,
      defaultEffort: 'medium',
      efforts: [
        { id: 'medium', label: 'Medium' },
        { id: 'high', label: 'High' }
      ]
    }
  ],
  permissionProfiles: ['ask', 'auto-edits'],
  collaborationModes: [
    { interactionMode: 'plan', wireMode: 'plan', reasoningEffort: 'medium' },
    { interactionMode: 'build', wireMode: 'default' }
  ]
}

describe('Codex live session catalog', () => {
  it('prepares one validated atomic task update', () => {
    expect(
      prepareCodexLiveSessionUpdate({
        catalog,
        current: { model: 'gpt-5.6-sol', effort: 'high' },
        update: { permission: 'auto-edits', interactionMode: 'plan' }
      })
    ).toMatchObject({
      currentModel: 'gpt-5.6-sol',
      runProfileUpdate: {
        permissions: ':workspace',
        approvalPolicy: 'on-request',
        collaborationMode: { mode: 'plan' }
      },
      accepted: {
        model: 'gpt-5.6-sol',
        effort: 'medium',
        permission: 'auto-edits',
        interactionMode: 'plan'
      }
    })
  })

  it('rejects unavailable permission profiles and model efforts', () => {
    expect(
      prepareCodexLiveSessionUpdate({
        catalog,
        update: { permission: 'bypass' }
      })
    ).toBeNull()
    expect(
      prepareCodexLiveSessionUpdate({
        catalog,
        update: { model: 'gpt-5.6-sol', effort: 'ultra' }
      })
    ).toBeNull()
  })
})
