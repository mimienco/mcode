import type { DevInstanceIdentity } from './dev-instance-identity'

type AppUserModelIdTarget = {
  setAppUserModelId: (id: string) => void
}

export function setWindowsAppUserModelId(
  target: AppUserModelIdTarget,
  identity: Pick<DevInstanceIdentity, 'appUserModelId'>,
  platform: NodeJS.Platform = process.platform
): void {
  if (platform === 'win32') {
    target.setAppUserModelId(identity.appUserModelId)
  }
}
