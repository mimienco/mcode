import { spawn, type ChildProcess } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { resolveGrokHomeDir } from '../../shared/grok-session-paths'
import { resolveCliCommand } from '../codex-cli/command'
import { getSpawnArgsForWindows } from '../win32-utils'

const GROK_REFRESH_TIMEOUT_MS = 30_000

type GrokAuthRefreshOptions = {
  signal?: AbortSignal
  command?: string
  cwd?: string
}

function resolveGrokCommand(): string {
  const home = resolveGrokHomeDir()
  const bundled = join(home, 'bin', process.platform === 'win32' ? 'grok.exe' : 'grok')
  return existsSync(bundled) ? bundled : resolveCliCommand('grok')
}

function refreshError(): Error {
  return new Error('Grok could not refresh its saved sign-in')
}

export function refreshGrokAuthWithCli(options: GrokAuthRefreshOptions = {}): Promise<void> {
  if (options.signal?.aborted) {
    return Promise.reject(refreshError())
  }

  return new Promise<void>((resolve, reject) => {
    const command = options.command ?? resolveGrokCommand()
    const { spawnCmd, spawnArgs } = getSpawnArgsForWindows(command, ['models'])
    let child: ChildProcess
    try {
      child = spawn(spawnCmd, spawnArgs, {
        cwd: options.cwd ?? resolveGrokHomeDir(),
        env: process.env,
        stdio: 'ignore',
        windowsHide: true
      })
    } catch {
      reject(refreshError())
      return
    }

    let settled = false
    const timeout = setTimeout(() => settle(false, true), GROK_REFRESH_TIMEOUT_MS)

    function cleanup(): void {
      clearTimeout(timeout)
      options.signal?.removeEventListener('abort', onAbort)
      child.off('error', onError)
      child.off('close', onClose)
    }

    function settle(success: boolean, kill = false): void {
      if (settled) {
        return
      }
      settled = true
      cleanup()
      if (kill) {
        child.kill()
      }
      if (success) {
        resolve()
      } else {
        reject(refreshError())
      }
    }

    function onAbort(): void {
      settle(false, true)
    }

    function onError(): void {
      settle(false)
    }

    function onClose(code: number | null): void {
      settle(code === 0)
    }

    options.signal?.addEventListener('abort', onAbort, { once: true })
    child.once('error', onError)
    child.once('close', onClose)
    // Close the narrow race where the caller aborts after the pre-spawn check
    // but before the event listener above is attached.
    if (options.signal?.aborted) {
      onAbort()
    }
  })
}
