import { isWindowsAbsolutePathLike } from './cross-platform-path'
import {
  isWslExecutable,
  isWslUncShareRootPath,
  nativeWindowsPathToPosixShellPath,
  nativeWindowsPathToWslShellPath,
  wslUncToLinuxPath
} from './wsl-paths'
import { quotePosixSafeChars, quoteWindowsCmdArg } from './shell-quoting'
import {
  buildWindowsCmdRunnerDelayedLaunchCommand,
  windowsRunnerPathNeedsCmdGuard
} from './windows-cmd-runner-delayed-launch'

export type SetupRunnerCommandPlatform = 'windows' | 'posix'
export type SetupRunnerShellFamily = 'posix' | 'cmd'
export type SetupRunnerCommandShell = 'posix' | 'windows'
export type SetupRunnerShell = {
  family: SetupRunnerShellFamily
  executable?: string
}

export type SetupRunnerCommandResolution = {
  command: string
  runnerScriptPathForShell: string
  shell: SetupRunnerCommandShell
}

export function buildSetupRunnerCommand(
  runnerScriptPath: string,
  platform: SetupRunnerCommandPlatform,
  shell?: SetupRunnerShell
): string {
  return resolveSetupRunnerCommand(runnerScriptPath, platform, shell).command
}

export function getSetupRunnerCommandPlatformForPath(
  runnerScriptPath: string,
  fallbackPlatform: SetupRunnerCommandPlatform
): SetupRunnerCommandPlatform {
  if (isWindowsAbsolutePathLike(runnerScriptPath)) {
    return 'windows'
  }
  if (runnerScriptPath.startsWith('/')) {
    return 'posix'
  }
  return fallbackPlatform
}

export function resolveSetupRunnerCommand(
  runnerScriptPath: string,
  platform: SetupRunnerCommandPlatform,
  shell?: SetupRunnerShell
): SetupRunnerCommandResolution {
  if (platform === 'windows') {
    if (isWslUncShareRootPath(runnerScriptPath)) {
      const linuxPath = wslUncToLinuxPath(runnerScriptPath)
      return {
        command: `bash ${quotePosixSafeChars(linuxPath)}`,
        runnerScriptPathForShell: linuxPath,
        shell: 'posix'
      }
    }
    if (runnerScriptPath.startsWith('/') && !isWindowsAbsolutePathLike(runnerScriptPath)) {
      return {
        command: `bash ${quotePosixSafeChars(runnerScriptPath)}`,
        runnerScriptPathForShell: runnerScriptPath,
        shell: 'posix'
      }
    }
    // Why: `shell` is the shell that types the command; the runner file's own extension decides
    // what can execute it. A batch runner never goes to bash even from a Git Bash pane.
    const cmdRunnerFile = isWindowsCmdRunnerPath(runnerScriptPath)
    if (!cmdRunnerFile && (shell?.family === 'posix' || /\.sh$/i.test(runnerScriptPath))) {
      // Why: WSL shells need /mnt/... paths, while Git Bash expects /c/... when replaying deferred setup scripts.
      if (isWslExecutable(shell?.executable)) {
        const wslPath = nativeWindowsPathToWslShellPath(runnerScriptPath)
        return {
          command: `bash ${quotePosixSafeChars(wslPath)}`,
          runnerScriptPathForShell: wslPath,
          shell: 'posix'
        }
      }
      // Why: queued setup launches can outlive the process that generated them, so convert native paths before handing off to POSIX shells.
      const posixPath = nativeWindowsPathToPosixShellPath(runnerScriptPath)
      return {
        command: `bash ${quotePosixSafeChars(posixPath)}`,
        runnerScriptPathForShell: posixPath,
        shell: 'posix'
      }
    }
    return {
      // Why: some path characters survive no amount of quoting on a cmd command line, and a Git
      // Bash pane rewrites the bare `/c` switch itself into a drive path (issue #6896) so cmd
      // opens interactively and the runner never starts. Both take the delayed-expansion
      // launcher, which passes the switch through a PowerShell ProcessStartInfo instead. Every
      // other case keeps the plain form.
      command:
        shell?.family === 'posix' || windowsRunnerPathNeedsCmdGuard(runnerScriptPath)
          ? buildWindowsCmdRunnerDelayedLaunchCommand(runnerScriptPath)
          : `cmd.exe /c ${quoteWindowsCmdArg(runnerScriptPath)}`,
      runnerScriptPathForShell: runnerScriptPath,
      shell: 'windows'
    }
  }

  return {
    command: `bash ${quotePosixSafeChars(runnerScriptPath)}`,
    runnerScriptPathForShell: runnerScriptPath,
    shell: 'posix'
  }
}

/** True when the runner file is a batch script, which only cmd can execute. */
export function isWindowsCmdRunnerPath(runnerScriptPath: string): boolean {
  return /\.(cmd|bat)$/i.test(runnerScriptPath)
}

