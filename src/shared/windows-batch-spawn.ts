import { win32 } from 'node:path'
import {
  UnsafeWindowsBatchArgumentsError,
  hasUnsafeWindowsBatchSyntax
} from './windows-batch-syntax'

export {
  UnsafeWindowsBatchArgumentsError,
  WINDOWS_BATCH_UNSAFE_ARGUMENTS_ERROR,
  WINDOWS_BATCH_UNSAFE_CHARACTERS_LABEL,
  hasUnsafeWindowsBatchSyntax
} from './windows-batch-syntax'

/** Full path to cmd.exe for GUI and service-launched processes. */
export function getCmdExePath(): string {
  return (
    process.env.ComSpec ||
    win32.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'cmd.exe')
  )
}

export function isWindowsBatchScript(commandPath: string): boolean {
  return process.platform === 'win32' && /\.(cmd|bat)$/i.test(commandPath)
}

export type GetSpawnArgsForWindowsOptions = {
  /**
   * GUI launchers (Open In apps) should not leave a lingering Command Prompt.
   * `start "" /B` returns immediately and keeps console-subsystem children of
   * `.cmd`/`.bat` shims from allocating a fresh visible prompt window.
   *
   * Opt-in only: `start` re-parses the command line, so callers whose argv can
   * carry quoted operands (VS Code `--remote` authorities and remote paths with
   * spaces) must leave this off.
   */
  detachedGui?: boolean
}

export function getSpawnArgsForWindows(
  command: string,
  args: string[],
  options: GetSpawnArgsForWindowsOptions = {}
): { spawnCmd: string; spawnArgs: string[] } {
  if (isWindowsBatchScript(command)) {
    for (const value of [command, ...args]) {
      if (hasUnsafeWindowsBatchSyntax(value)) {
        throw new UnsafeWindowsBatchArgumentsError()
      }
    }

    // Why: separate argv entries let Node quote spaces without breaking cmd.
    if (options.detachedGui) {
      // Why: `start` launches a batch target through a nested `cmd /K`, which
      // stays resident after the script ends — `/B` only suppresses a *new*
      // console, so the shim leaks a hidden cmd.exe. Handing `start` an inner
      // `cmd /d /c` makes that interpreter exit with the script.
      //
      // Window title must be an *empty argv entry* (`''`). libuv's Windows
      // quoter turns empty into `""` on the CreateProcess command line — the
      // empty title `start` requires so a later quoted path is not eaten as
      // the title. The two-character string `'""'` is wrong: libuv re-escapes
      // it to `"\"\""`. (Default ComSpec has no spaces, so the bad form often
      // still "works"; quoted Program Files paths are where it breaks.)
      const cmdExePath = getCmdExePath()
      return {
        spawnCmd: cmdExePath,
        spawnArgs: ['/d', '/c', 'start', '', '/B', cmdExePath, '/d', '/c', command, ...args]
      }
    }
    return { spawnCmd: getCmdExePath(), spawnArgs: ['/d', '/c', command, ...args] }
  }
  return { spawnCmd: command, spawnArgs: args }
}
