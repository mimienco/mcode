import type { OnboardingApi } from '../../../preload/api/onboarding-api'
import { getDefaultOnboardingState } from '../../../shared/constants'
import type { OnboardingState } from '../../../shared/onboarding-state-types'
import type { SettingsDocumentBridge } from './tauri-settings-document-client'
import { DesktopCommandError } from './desktop-client'

function readState(document: Record<string, unknown>): OnboardingState {
  const defaults = getDefaultOnboardingState()
  const value = document.onboarding
  if (value === undefined || value === null) {
    return defaults
  }
  if (typeof value !== 'object' || Array.isArray(value)) {
    throw new DesktopCommandError('onboarding_invalid_state', { operation: 'onboarding.get' },
      'Persisted onboarding state is invalid.', 'MCode could not read onboarding progress.')
  }
  const fields = value as Partial<OnboardingState>
  return { ...defaults, ...fields, checklist: { ...defaults.checklist, ...fields.checklist } }
}

export function createTauriOnboardingClient(bridge: SettingsDocumentBridge): OnboardingApi {
  return {
    get: async () => readState(await bridge.getDocument()),
    update: async (updates) => {
      // Send only changed fields: the native atomic merge preserves concurrent checklist writes.
      return readState(await bridge.setDocument({ onboarding: updates }))
    }
  }
}
