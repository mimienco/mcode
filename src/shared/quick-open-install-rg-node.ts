import { readNodeFileWithinLimit } from './node-bounded-file-reader'
import {
  detectInstallCommand,
  detectLinuxInstallCommandFromOsRelease,
  GENERIC_LINUX_RIPGREP_INSTALL
} from './quick-open-install-rg'

// Node-side companion to quick-open-install-rg.ts: the distro-refined install
// command and the user-facing rg guidance messages. Main and relay import this
// module; it must never enter the renderer graph (node:fs).

const MAX_OS_RELEASE_BYTES = 64 * 1024

async function detectInstallCommandWithDistroRefinement(): Promise<string> {
  if (process.platform !== 'linux') {
    return detectInstallCommand(process.platform)
  }
  try {
    const osRelease = (
      await readNodeFileWithinLimit('/etc/os-release', MAX_OS_RELEASE_BYTES)
    ).buffer.toString('utf8')
    return detectLinuxInstallCommandFromOsRelease(osRelease)
  } catch {
    return GENERIC_LINUX_RIPGREP_INSTALL
  }
}

export async function buildInstallRgMessage(
  cause: unknown,
  host: 'local' | 'remote' = 'local'
): Promise<string> {
  const reason = cause instanceof Error ? cause.message : String(cause)
  const cmd = await detectInstallCommandWithDistroRefinement()
  const location = host === 'local' ? 'on the host running the Quick Open scan' : 'on the remote'
  return (
    `Quick Open scan too large (${reason}). ` +
    `Install ripgrep ${location} to enable fast, gitignore-aware listing: ${cmd}`
  )
}

export async function buildRipgrepRequiredMessage(
  host: 'local' | 'remote' = 'local'
): Promise<string> {
  const cmd = await detectInstallCommandWithDistroRefinement()
  const location = host === 'local' ? 'on the host running Quick Open' : 'on the remote'
  return `Quick Open search requires ripgrep ${location} to stay resource-bounded. Install it with: ${cmd}`
}
