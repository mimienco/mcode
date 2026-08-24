/**
 * Pure search-and-replace rewriting shared by the local main process and the
 * SSH relay. Matching semantics intentionally reuse `buildSubmatchRegex` so the
 * count Replace All reports always agrees with what Search highlighted.
 */
import type {
  PreserveCaseMode,
  ReplaceFileOutcome,
  ReplaceOptions,
  ReplaceSummary
} from './code-replace-types'

const BOM = '\uFEFF'

/** The query cannot be rewritten precisely; callers must fail closed. */
export class ReplaceQueryError extends Error {}

function escapeLiteralForRegex(query: string): string {
  return query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * Build the validated global regex for a replace run.
 *
 * Mirrors search matching exactly (same builder as submatch highlighting), so
 * "Replace All" never rewrites a span Search did not highlight.
 *
 * @throws ReplaceQueryError for an empty query, an always-empty match
 * (`a*`, `\b`, `^` — would insert text everywhere), or a regex Search can
 * highlight but JS cannot rewrite (POSIX classes).
 */
export function createReplaceRegex(query: string, opts: ReplaceOptions): RegExp {
  if (query.length === 0) {
    throw new ReplaceQueryError('Empty query')
  }
  // Why: JS compiles POSIX classes as an unrelated character class, so rg would
  // highlight different spans than this regex rewrites.
  if (opts.useRegex && /\[:[a-z]+:\]/.test(query)) {
    throw new ReplaceQueryError('POSIX character classes are not supported')
  }
  const source = opts.useRegex ? query : escapeLiteralForRegex(query)
  let regex: RegExp | null = null
  try {
    regex = new RegExp(opts.wholeWord ? `\\b(?:${source})\\b` : source, `g${opts.caseSensitive ? '' : 'i'}`)
  } catch {
    throw new ReplaceQueryError('Query is not a valid JavaScript regular expression')
  }
  // Why: zero-length matches would insert replacement text between every character.
  if (regex.test('')) {
    throw new ReplaceQueryError('Query can match empty text')
  }
  regex.lastIndex = 0
  return regex
}

/**
 * Expand `$` tokens in user replacement text against one match: `$$`, `$&`,
 * `$1`–`$9`. Unknown tokens stay literal.
 */
export function expandReplacement(replacement: string, match: RegExpExecArray): string {
  let out = ''
  let i = 0
  while (i < replacement.length) {
    const ch = replacement[i]
    if (ch !== '$' || i + 1 >= replacement.length) {
      out += ch
      i++
      continue
    }
    const next = replacement[i + 1]
    if (next === '$') {
      out += '$'
      i += 2
      continue
    }
    if (next === '&') {
      out += match[0]
      i += 2
      continue
    }
    if (next >= '1' && next <= '9') {
      const group = match[Number(next)]
      if (group !== undefined) {
        out += group
        i += 2
        continue
      }
    }
    out += ch
    i++
  }
  return out
}

function hasLetters(value: string): boolean {
  return /[^\W\d_]/.test(value)
}

/** `Foo` / `Foo bar` — first character upper, remainder lower. */
function isCapitalized(value: string): boolean {
  const lowered = value.toLowerCase()
  return value === lowered.charAt(0).toUpperCase() + lowered.slice(1)
}

/**
 * Case the expanded replacement the way the matched text was cased:
 * `foo`→as typed, `FOO`→UPPER, `Foo`→Capitalized, mixed stays as typed.
 */
export function applyMatchCasing(replacement: string, matchedText: string): string {
  if (!hasLetters(matchedText)) {
    return replacement
  }
  if (matchedText === matchedText.toUpperCase()) {
    return replacement.toUpperCase()
  }
  if (isCapitalized(matchedText)) {
    return replacement.charAt(0).toUpperCase() + replacement.slice(1)
  }
  return replacement
}

function splitKeepingTerminators(text: string): string[] {
  return text.split(/(?<=\n)/)
}

function splitTerminator(line: string): { body: string; terminator: string } {
  if (line.endsWith('\r\n')) {
    return { body: line.slice(0, -2), terminator: '\r\n' }
  }
  if (line.endsWith('\n')) {
    return { body: line.slice(0, -1), terminator: '\n' }
  }
  return { body: line, terminator: '' }
}

export type FileReplaceResult = { content: string; occurrences: number }

/**
 * Rewrite every match in one file's text, line by line so CRLF/LF terminators
 * survive byte-for-byte even in mixed-ending files.
 *
 * @returns null when nothing matched (callers skip the write), else the new
 * content plus the occurrence count. Occurrences cover the whole file, not
 * just what a truncated search displayed.
 */
export function replaceFileContent(
  text: string,
  regex: RegExp,
  replacement: string,
  opts: { preserveCase?: PreserveCaseMode }
): FileReplaceResult | null {
  const hadBom = text.startsWith(BOM)
  const body = hadBom ? text.slice(BOM.length) : text
  const lines = splitKeepingTerminators(body)
  let occurrences = 0

  const rebuilt = lines.map((line) => {
    const { body: lineBody, terminator } = splitTerminator(line)
    if (!lineBody) {
      return line
    }
    regex.lastIndex = 0
    let rewritten = ''
    let cursor = 0
    let m: RegExpExecArray | null
    while ((m = regex.exec(lineBody)) !== null) {
      // Guard against zero-length loops; createReplaceRegex rejects the common
      // cases up front, lazy patterns can still slip through here.
      if (m[0].length === 0) {
        regex.lastIndex++
        continue
      }
      let expanded = expandReplacement(replacement, m)
      if (opts.preserveCase === 'match') {
        expanded = applyMatchCasing(expanded, m[0])
      }
      rewritten += lineBody.slice(cursor, m.index) + expanded
      cursor = m.index + m[0].length
      occurrences++
    }
    if (cursor === 0) {
      return line
    }
    return rewritten + lineBody.slice(cursor) + terminator
  })

  if (occurrences === 0) {
    return null
  }
  return { content: (hadBom ? BOM : '') + rebuilt.join(''), occurrences }
}

export function summarizeReplaceOutcomes(outcomes: ReplaceFileOutcome[]): ReplaceSummary {
  return {
    outcomes,
    writtenFiles: outcomes.filter((o) => o.status === 'written').length,
    writtenOccurrences: outcomes.reduce((sum, o) => sum + (o.occurrences ?? 0), 0),
    skippedFiles: outcomes.filter((o) => o.status === 'skipped' && o.reason !== 'unchanged')
      .length,
    failedFiles: outcomes.filter((o) => o.status === 'failed').length
  }
}
