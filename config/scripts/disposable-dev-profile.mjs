import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

const profileRoot = path.resolve(tmpdir())

function isDisposableProfilePath(candidate) {
  const resolved = path.resolve(candidate)
  const relative = path.relative(profileRoot, resolved)
  return (
    relative !== '' &&
    relative !== '..' &&
    !relative.startsWith(`..${path.sep}`) &&
    !path.isAbsolute(relative) &&
    path.basename(resolved).startsWith('mcode-dev-')
  )
}

export function createDisposableDevProfile() {
  const profilePath = mkdtempSync(path.join(profileRoot, 'mcode-dev-'))
  if (!isDisposableProfilePath(profilePath)) {
    throw new Error(`Refusing unexpected fresh dev profile path: ${profilePath}`)
  }

  return {
    profilePath,
    cleanup() {
      if (!isDisposableProfilePath(profilePath)) {
        console.warn(`[mcode-dev] Refusing to remove unexpected profile path: ${profilePath}`)
        return
      }
      try {
        rmSync(profilePath, { recursive: true, force: true, maxRetries: 3, retryDelay: 50 })
      } catch (error) {
        console.warn(
          `[mcode-dev] Could not remove disposable profile ${profilePath}: ${error?.message ?? error}`
        )
      }
    }
  }
}
