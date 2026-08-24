import { describe, expect, it } from 'vitest'
import {
  buildCodexRunProfileSettingsUpdate,
  filterCodexPermissionProfilesByRequirements,
  parseCodexCollaborationModes,
  parseCodexPermissionProfiles,
  parseCodexThreadRunProfile
} from './codex-live-session-controls'

describe('Codex live session controls', () => {
  it('keeps only allowed MCode permission profiles and verified Plan/Build presets', () => {
    expect(
      parseCodexPermissionProfiles({
        data: [
          { id: ':read-only', allowed: true },
          { id: ':workspace', allowed: true },
          { id: ':danger-full-access', allowed: false },
          { id: 'custom', allowed: true }
        ]
      })
    ).toEqual(['ask', 'auto-edits'])
    expect(
      parseCodexCollaborationModes({
        data: [
          { name: 'Plan', mode: 'plan', reasoning_effort: 'medium' },
          { name: 'Default', mode: 'default' }
        ]
      })
    ).toEqual([
      { interactionMode: 'plan', wireMode: 'plan', reasoningEffort: 'medium' },
      { interactionMode: 'build', wireMode: 'default' }
    ])
  })

  it('filters permission profiles through admin approval and sandbox allowlists', () => {
    expect(
      filterCodexPermissionProfilesByRequirements(['ask', 'auto-edits', 'bypass'], {
        requirements: {
          allowedApprovalPolicies: ['onRequest'],
          allowedSandboxModes: ['readOnly', 'workspaceWrite']
        }
      })
    ).toEqual(['ask', 'auto-edits'])
    expect(
      filterCodexPermissionProfilesByRequirements(['ask', 'bypass'], {
        requirements: null
      })
    ).toEqual(['ask', 'bypass'])
  })

  it('builds an atomic permission and collaboration update', () => {
    expect(
      buildCodexRunProfileSettingsUpdate({
        permission: 'auto-edits',
        interactionMode: 'plan',
        currentModel: 'gpt-5.6-sol',
        currentEffort: 'high',
        collaborationModes: [
          { interactionMode: 'plan', wireMode: 'plan', reasoningEffort: 'medium' }
        ]
      })
    ).toEqual({
      permissions: ':workspace',
      approvalPolicy: 'on-request',
      collaborationMode: {
        mode: 'plan',
        settings: {
          model: 'gpt-5.6-sol',
          reasoning_effort: 'medium',
          developer_instructions: null
        }
      }
    })
  })

  it('reconciles provider-reported permission and mode state', () => {
    expect(
      parseCodexThreadRunProfile({
        activePermissionProfile: { id: ':danger-full-access', extends: null },
        approvalPolicy: 'never',
        collaborationMode: { mode: 'default' }
      })
    ).toEqual({ permission: 'bypass', interactionMode: 'build' })
  })

  it('does not overclaim bypass from a no-prompt policy without sandbox evidence', () => {
    expect(parseCodexThreadRunProfile({ approvalPolicy: 'never' })).toEqual({})
    expect(
      parseCodexThreadRunProfile({
        sandboxPolicy: { type: 'workspaceWrite' },
        collaborationMode: 'plan'
      })
    ).toEqual({ permission: 'auto-edits', interactionMode: 'plan' })
  })
})
