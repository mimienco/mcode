import { describe, expect, it } from 'vitest'
import { createTauriOnboardingClient } from './tauri-onboarding-client'
import { createSettingsDocumentBridge } from './tauri-settings-document-client'
import { getDefaultOnboardingState } from '../../../shared/constants'

describe('Tauri onboarding persistence', () => {
  it('hydrates a missing section with the real first-run defaults', async () => {
    const client = createTauriOnboardingClient(createSettingsDocumentBridge(async () => ({})))
    expect(await client.get()).toEqual(getDefaultOnboardingState())
  })

  it('preserves dismissed/completed fields and fills only missing checklist fields', async () => {
    const client = createTauriOnboardingClient(createSettingsDocumentBridge(async () => ({
      onboarding: { closedAt: 42, outcome: 'dismissed', lastCompletedStep: 3, checklist: { addedRepo: true, dismissed: true } }
    })))
    expect(await client.get()).toMatchObject({ closedAt: 42, outcome: 'dismissed', lastCompletedStep: 3,
      checklist: { addedRepo: true, addedFolder: false, dismissed: true } })
  })

  it('writes only the supplied delta and retains explicit null when reopening onboarding', async () => {
    const calls: unknown[] = []
    const client = createTauriOnboardingClient(createSettingsDocumentBridge(async (command, args) => {
      calls.push({ command, args })
      return { onboarding: { closedAt: null, outcome: null, lastCompletedStep: 4, checklist: { addedRepo: true, addedFolder: true } } }
    }))
    const result = await client.update({ closedAt: null, outcome: null, checklist: { addedFolder: true } })
    expect(calls).toEqual([{ command: 'settings_set_document', args: { patch: { onboarding: { closedAt: null, outcome: null, checklist: { addedFolder: true } } } } }])
    expect(result).toMatchObject({ closedAt: null, outcome: null, lastCompletedStep: 4, checklist: { addedRepo: true, addedFolder: true } })
  })

  it('rejects corrupt persisted state instead of resetting progress', async () => {
    const client = createTauriOnboardingClient(createSettingsDocumentBridge(async () => ({ onboarding: false })))
    await expect(client.get()).rejects.toMatchObject({ code: 'onboarding_invalid_state' })
  })
})
