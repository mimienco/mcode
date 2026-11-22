import { quoteCmdToken } from '../../shared/shell-quoting'
export type WindowsCommandInvocation = {
  command: string
  args: string[]
  windowsVerbatimArguments: true
}

export function buildWindowsCommandInvocation(
  command: string,
  args: string[],
  commandInterpreter = process.env.ComSpec ?? 'cmd.exe'
): WindowsCommandInvocation {
  const commandLine = [command, ...args].map(quoteCmdToken).join(' ')
  return {
    command: commandInterpreter,
    args: ['/d', '/v:off', '/s', '/c', `"${commandLine}"`],
    windowsVerbatimArguments: true
  }
}
