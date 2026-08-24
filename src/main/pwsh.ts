import { resolveWindowsPowerShellExecutablePath } from './providers/windows-powershell-executable'

const PWSH_NEGATIVE_CACHE_TTL_MS = 30_000

type PwshAvailabilityCache =
  | { available: true }
  | { available: false; cachedAt: number }

let pwshAvailableCache: PwshAvailabilityCache | null = null

function isCacheFresh(cache: PwshAvailabilityCache): boolean {
  return cache.available || Date.now() - cache.cachedAt < PWSH_NEGATIVE_CACHE_TTL_MS
}

function probePwshAvailability(): boolean {
  return resolveWindowsPowerShellExecutablePath('pwsh.exe') !== null
}

/**
 * Check whether PowerShell 7 can back a Windows terminal.
 *
 * The ConPTY resolver already validates PATH, standard install locations and
 * Store aliases. Reusing it avoids starting pwsh.exe merely to learn whether
 * it can be started — process creation can hold Electron's main thread for
 * seconds on a cold or AV-busy Windows launch.
 */
export function isPwshAvailable(): boolean {
  if (pwshAvailableCache && isCacheFresh(pwshAvailableCache)) {
    return pwshAvailableCache.available
  }
  if (process.platform !== 'win32') {
    pwshAvailableCache = { available: false, cachedAt: Date.now() }
    return false
  }

  const available = probePwshAvailability()
  pwshAvailableCache = available ? { available: true } : { available: false, cachedAt: Date.now() }
  return available
}

export function isPwshAvailableAsync(): Promise<boolean> {
  return Promise.resolve(isPwshAvailable())
}

export function warmPwshAvailabilityCache(): Promise<boolean> {
  return Promise.resolve(isPwshAvailable())
}
