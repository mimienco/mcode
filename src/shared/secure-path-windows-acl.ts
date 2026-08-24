import { execFile, execFileSync } from 'node:child_process'
import { win32 as pathWin32 } from 'node:path'

let cachedWindowsUserSid: string | null | undefined

// ponytail: /inheritance:r strips inherited ACEs but leaves pre-existing explicit third-party ACEs
// in place (the old PowerShell script removed those too). Our secure files are always freshly created
// tmp files with no explicit ACEs, so this only matters for hardenExistingSecureFile on externally-
// modified files; upgrade path is a post-grant `icacls /verify`-style audit if that ever becomes real.
function buildIcaclsArgs(
  targetPath: string,
  currentUserSid: string,
  isDirectory: boolean
): string[] {
  const inherit = isDirectory ? '(OI)(CI)' : ''
  return [
    targetPath,
    '/inheritance:r',
    '/grant:r',
    `*${currentUserSid}:${inherit}F`,
    `*S-1-5-18:${inherit}F`, // SYSTEM
    `*S-1-5-32-544:${inherit}F` // Administrators
  ]
}

export function bestEffortRestrictWindowsPath(targetPath: string, isDirectory: boolean): void {
  const currentUserSid = getCurrentWindowsUserSid()
  if (!currentUserSid) {
    return
  }
  // Why: async to avoid blocking the main thread — hardening runs on the frequent read path (#4901).
  execFile(
    getWindowsSystemToolPath('icacls.exe'),
    buildIcaclsArgs(targetPath, currentUserSid, isDirectory),
    {
      windowsHide: true,
      timeout: 5000
    },
    () => {
      // Why: ignore errors — hardening is best-effort; icacls may be unavailable or locked down.
    }
  )
}

export function restrictWindowsPathSync(targetPath: string, isDirectory: boolean): boolean {
  const currentUserSid = getCurrentWindowsUserSid()
  if (!currentUserSid) {
    return false
  }
  // Why: file must not be published until its ACL is actually restricted, so block and report real success (read path stays async, #4901).
  try {
    execFileSync(getWindowsSystemToolPath('icacls.exe'), buildIcaclsArgs(targetPath, currentUserSid, isDirectory), {
      stdio: ['ignore', 'ignore', 'ignore'],
      windowsHide: true,
      timeout: 5000
    })
    return true
  } catch {
    // Why: best-effort — a failed ACL apply must not crash the write; false leaves the path uncached to retry later.
    return false
  }
}

function getCurrentWindowsUserSid(): string | null {
  if (cachedWindowsUserSid !== undefined) {
    return cachedWindowsUserSid
  }
  try {
    const output = execFileSync(
      getWindowsSystemToolPath('whoami.exe'),
      ['/user', '/fo', 'csv', '/nh'],
      {
        encoding: 'utf-8',
        stdio: ['ignore', 'pipe', 'ignore'],
        windowsHide: true,
        timeout: 5000
      }
    ).trim()
    const columns = parseCsvLine(output)
    cachedWindowsUserSid = columns[1] ?? null
  } catch {
    cachedWindowsUserSid = null
  }
  return cachedWindowsUserSid
}

function getWindowsSystemToolPath(relativeSystem32Path: string): string {
  const systemRoot = process.env.SystemRoot || process.env.WINDIR || 'C:\\Windows'
  return pathWin32.join(systemRoot, 'System32', relativeSystem32Path)
}

function parseCsvLine(line: string): string[] {
  return line.split(/","/).map((part) => part.replace(/^"/, '').replace(/"$/, ''))
}

export function resetSecureFileWindowsUserSidForTests(): void {
  cachedWindowsUserSid = undefined
}
