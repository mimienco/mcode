import { describe, expect, it } from 'vitest'
import {
  applyMatchCasing,
  createReplaceRegex,
  expandReplacement,
  ReplaceQueryError,
  replaceFileContent
} from './text-replace'

describe('createReplaceRegex', () => {
  it('treats literal queries as fixed strings', () => {
    const regex = createReplaceRegex('a.c', {})
    expect('a.c abc'.replace(regex, 'X')).toBe('X abc')
  })

  it('defaults to case-insensitive and honors caseSensitive', () => {
    expect(createReplaceRegex('foo', {}).test('FOO')).toBe(true)
    expect(createReplaceRegex('foo', { caseSensitive: true }).test('FOO')).toBe(false)
  })

  it('wraps wholeWord without swallowing adjacent words', () => {
    const regex = createReplaceRegex('foo', { wholeWord: true })
    expect('foo foobar'.replace(regex, 'X')).toBe('X foobar')
  })

  it('keeps user alternations inside the word boundary', () => {
    const regex = createReplaceRegex('foo|bar', { useRegex: true, wholeWord: true })
    expect('foo xbar yfoobarz'.replace(regex, 'X')).toBe('X xbar yfoobarz')
  })

  it('rejects empty queries', () => {
    expect(() => createReplaceRegex('', {})).toThrow(ReplaceQueryError)
  })

  it.each(['a*', 'a*?', '^', '(x)?'])('rejects always-empty pattern %s', (pattern) => {
    expect(() => createReplaceRegex(pattern, { useRegex: true })).toThrow(ReplaceQueryError)
  })

  it('allows \\b because zero-width hits are skipped, not inserted', () => {
    const regex = createReplaceRegex('\\bfoo\\b\\d*', { useRegex: true })
    expect('foo foobar'.replace(regex, 'X')).toBe('X foobar')
  })
})

describe('expandReplacement', () => {
  const match = ['abc', 'a', 'bc'] as unknown as RegExpExecArray

  it('expands whole match, groups, and escaped dollar', () => {
    expect(expandReplacement('$&!$1-$2', match)).toBe('abc!a-bc')
  })

  it('collapses $$ into a literal dollar', () => {
    expect(expandReplacement('$$1', match)).toBe('$1')
  })

  it('leaves unknown tokens and missing groups literal', () => {
    expect(expandReplacement('$9$x $3', match)).toBe('$9$x $3')
  })

  it('reads only one digit after a group token', () => {
    const twoGroups = ['ab12', 'ab', '12'] as unknown as RegExpExecArray
    expect(expandReplacement('$12', twoGroups)).toBe('ab2')
  })
})

describe('applyMatchCasing', () => {
  it('keeps lower-case samples as typed', () => {
    expect(applyMatchCasing('bar', 'foo')).toBe('bar')
  })

  it('upper-cases for upper samples including punctuation', () => {
    expect(applyMatchCasing('bar', 'FOO')).toBe('BAR')
    expect(applyMatchCasing('bar_baz', 'FOO_QUX')).toBe('BAR_BAZ')
  })

  it('capitalizes for capitalized samples', () => {
    expect(applyMatchCasing('bar', 'Foo')).toBe('Bar')
    expect(applyMatchCasing('bar baz', 'Foo qux')).toBe('Bar baz')
  })

  it('leaves mixed and letter-less samples as typed', () => {
    expect(applyMatchCasing('bar', 'fOO')).toBe('bar')
    expect(applyMatchCasing('bar', '123')).toBe('bar')
  })
})

describe('replaceFileContent', () => {
  const make = (query: string, replacement: string, opts = {}) =>
    replaceFileContent(text, createReplaceRegex(query, opts), replacement, opts)

  let text: string

  it('replaces every occurrence across lines and counts them', () => {
    text = 'foo bar\nbaz foo\nfoo'
    expect(make('foo', 'qux')).toEqual({ content: 'qux bar\nbaz qux\nqux', occurrences: 3 })
  })

  it('returns null when nothing matches', () => {
    text = 'nothing here'
    expect(make('zzz', 'x')).toBeNull()
  })

  it('preserves CRLF, LF, and mixed terminators per line', () => {
    text = 'a=foo\r\nb=foo\nc=foo\r\n'
    expect(make('foo', 'bar')?.content).toBe('a=bar\r\nb=bar\nc=bar\r\n')
  })

  it('preserves a missing final newline', () => {
    text = 'foo\nlast foo'
    expect(make('foo', 'x')?.content).toBe('x\nlast x')
  })

  it('keeps the BOM and never matches through it', () => {
    text = '\uFEFFfoo'
    expect(make('foo', 'bar')?.content).toBe('\uFEFFbar')
    expect(make('\uFEFFfoo', 'bar')).toBeNull()
  })

  it('reorders regex capture groups', () => {
    text = 'milan@example.com'
    const out = replaceFileContent(
      text,
      createReplaceRegex('(\\w+)@(\\w+\\.\\w+)', { useRegex: true }),
      '$2:$1',
      {}
    )
    expect(out).toEqual({ content: 'example.com:milan', occurrences: 1 })
  })

  it('applies preserveCase per occurrence', () => {
    text = 'foo Foo FOO'
    expect(make('foo', 'bar', { preserveCase: 'match' })?.content).toBe('bar Bar BAR')
  })

  it('skips zero-width matches without phantom counts', () => {
    text = 'bcd aad'
    expect(make('\\ba*', 'X', { useRegex: true })).toEqual({ content: 'bcd Xd', occurrences: 1 })
  })

  it('survives unicode neighbors byte-for-byte outside matches', () => {
    text = 'éé foo 🚀 foo'
    expect(make('foo', 'βαρ')?.content).toBe('éé βαρ 🚀 βαρ')
  })
})
