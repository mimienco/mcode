import { POSIX_HOOK_STDIN_DRAIN_COMMAND } from './hook-stdin-contract'
import { quotePosixSingleQuote } from '../../shared/shell-quoting'

// Why: guard for a readable executable so a stale entry at a missing script becomes a silent no-op, not an exit-127 failure on every tool call.
export function wrapPosixHookCommand(
  scriptPath: string,
  env: Record<string, string> = {},
  // Why: silence is a hard deny on gate events (Antigravity PreToolUse, #2426); those callers need the guard to still answer.
  options: { fallbackStdout?: string } = {}
): string {
  // Why: single-quote escape so $, `, ", \ in scriptPath stay literal — avoids shell injection from an arbitrary path.
  const quoted = quotePosixSingleQuote(scriptPath)
  const envPrefix = Object.entries(env)
    .map(([key, value]) => `${key}=${quotePosixSingleQuote(value)}`)
    .join(' ')
  const invocation = envPrefix ? `${envPrefix} /bin/sh ${quoted}` : `/bin/sh ${quoted}`
  const fallback =
    options.fallbackStdout === undefined
      ? POSIX_HOOK_STDIN_DRAIN_COMMAND
      : `printf '%s\\n' ${quotePosixSingleQuote(options.fallbackStdout)}; ${POSIX_HOOK_STDIN_DRAIN_COMMAND}`
  return `if [ -f ${quoted} ] && [ -r ${quoted} ] && [ -x ${quoted} ]; then ${invocation}; else ${fallback}; fi`
}
