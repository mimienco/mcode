import {
  WINDOWS_BATCH_UNSAFE_ARGUMENTS_ERROR,
  hasUnsafeWindowsBatchSyntax
} from './windows-batch-syntax'

/**
 * POSIX single-quote, always quoted. The lone metacharacter `'` is escaped by
 * closing the quote, inserting an escaped literal, and reopening: `'\''`.
 */
export function quotePosixSingleQuote(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`
}

/**
 * POSIX single-quote with a safe-character fast path: values made only of
 * `[A-Za-z0-9_./:-]` are emitted unquoted. Kept separate from
 * {@link quotePosixSingleQuote} because callers embed the output in
 * human-visible command lines where the unquoted form matters.
 */
export function quotePosixSafeChars(value: string): string {
  if (/^[A-Za-z0-9_./:-]+$/.test(value)) {
    return value
  }
  return `'${value.replace(/'/g, `'\\''`)}'`
}

/**
 * POSIX single-quote using the `'"'"'` splice instead of the `'\''` escape.
 * Byte-incompatible with {@link quotePosixSingleQuote}: scripts that parse our
 * output back (e.g. the managed WSL launcher's `MCODE_WIN_LAUNCHER` value)
 * match this exact spelling, so the dialects must not be unified.
 */
export function quotePosixQuoteSplice(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`
}

/**
 * Windows cmd.exe argument: wrap in double quotes and double embedded quotes.
 */
export function quoteWindowsCmdArg(value: string): string {
  return `"${value.replace(/"/g, '""')}"`
}

/**
 * PowerShell string literal: wrap in single quotes and double embedded quotes.
 */
export function quotePowerShellString(value: string): string {
  return `'${value.replaceAll("'", "''")}'`
}

/**
 * One spawnable Windows command token at the CRT argv level: rejects quotes
 * and line breaks outright, doubles trailing backslashes ahead of the closing
 * quote, and escapes `%` by leaving the quoted span (`"^%"`) so cmd variable
 * expansion cannot run inside it. Not interchangeable with
 * {@link quoteWindowsCmdArg}.
 */
export function quoteCmdToken(value: string): string {
  if (/[\r\n"]/.test(value)) {
    throw new Error('Windows command tokens cannot contain quotes or line breaks.')
  }
  const crtEscaped = value.replace(
    /(\\*)$/,
    (_match, backslashes: string) => `${backslashes}${backslashes}`
  )
  return `"${crtEscaped.replace(/%/g, '"^%"')}"`
}

/**
 * One token on a cmd.exe-parsed batch command line: rejects the characters
 * that let cmd chain commands or expand variables, else wraps in double
 * quotes. Throws the same sentinel error the batch spawn guard uses.
 */
export function quoteWindowsBatchToken(value: string): string {
  if (hasUnsafeWindowsBatchSyntax(value)) {
    throw new Error(WINDOWS_BATCH_UNSAFE_ARGUMENTS_ERROR)
  }
  return `"${value}"`
}
