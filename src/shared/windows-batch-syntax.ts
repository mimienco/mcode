export const WINDOWS_BATCH_UNSAFE_ARGUMENTS_ERROR = 'UNSAFE_WINDOWS_BATCH_ARGUMENTS'

export class UnsafeWindowsBatchArgumentsError extends Error {
  constructor() {
    super(WINDOWS_BATCH_UNSAFE_ARGUMENTS_ERROR)
    this.name = 'UnsafeWindowsBatchArgumentsError'
  }
}

// Why: cmd.exe re-parses the command line, and these are the characters that can
// start a new command or expand a variable out of an otherwise inert argument.
// `(`/`)` are deliberately absent: they only group commands, and grouping cannot
// chain anything without one of the separators below, so rejecting them merely
// broke every `C:\Program Files (x86)\...` shim and paren-bearing worktree path.
const WINDOWS_BATCH_UNSAFE_CHARACTERS = ['&', '|', '<', '>', '^', '"', '%', '!'] as const

/** The rejected characters, spelled for error messages so they cannot drift from the guard. */
export const WINDOWS_BATCH_UNSAFE_CHARACTERS_LABEL = WINDOWS_BATCH_UNSAFE_CHARACTERS.join(' ')

const UNSAFE_WINDOWS_BATCH_SYNTAX = new RegExp(
  `[${WINDOWS_BATCH_UNSAFE_CHARACTERS.map((character) => character.replace(/[\\^\]-]/, '\\$&')).join('')}\\r\\n]`
)

export function hasUnsafeWindowsBatchSyntax(value: string): boolean {
  return UNSAFE_WINDOWS_BATCH_SYNTAX.test(value)
}
