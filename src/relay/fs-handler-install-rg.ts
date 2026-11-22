import {
  buildInstallRgMessage as buildSharedInstallRgMessage,
  buildRipgrepRequiredMessage as buildSharedRipgrepRequiredMessage
} from '../shared/quick-open-install-rg-node'
import { detectInstallCommand, detectLinuxInstallCommandFromOsRelease } from '../shared/quick-open-install-rg'

export { detectInstallCommand, detectLinuxInstallCommandFromOsRelease }

export function buildInstallRgMessage(cause: unknown): Promise<string> {
  return buildSharedInstallRgMessage(cause, 'remote')
}

export function buildRipgrepRequiredMessage(): Promise<string> {
  return buildSharedRipgrepRequiredMessage('remote')
}
