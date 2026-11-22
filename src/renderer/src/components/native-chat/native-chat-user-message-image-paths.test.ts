import { describe, expect, it } from 'vitest'
import { extractUserMessageImagePaths } from './native-chat-user-message-image-paths'

describe('extractUserMessageImagePaths', () => {
  it('strips an @-prefixed quoted Windows path and keeps the remaining question text', () => {
    const result = extractUserMessageImagePaths(
      '@"C:\\Users\\Milan M\\Downloads\\HQVyLpbkAAsoad.jfif" was ist aufs bild'
    )

    expect(result.imagePaths).toEqual(['C:\\Users\\Milan M\\Downloads\\HQVyLpbkAAsoad.jfif'])
    expect(result.text).toBe('was ist aufs bild')
  })

  it('strips a quoted POSIX path without the @ prefix', () => {
    const result = extractUserMessageImagePaths('"/posix/shots/x.png" look at this')

    expect(result.imagePaths).toEqual(['/posix/shots/x.png'])
    expect(result.text).toBe('look at this')
  })

  it('strips a bare absolute path at the start of the message', () => {
    const result = extractUserMessageImagePaths('/home/me/photo.jpg thanks')

    expect(result.imagePaths).toEqual(['/home/me/photo.jpg'])
    expect(result.text).toBe('thanks')
  })

  it('strips a bare Windows drive path at the end of the message', () => {
    const result = extractUserMessageImagePaths('check this C:\\shots\\pic.webp')

    expect(result.imagePaths).toEqual(['C:\\shots\\pic.webp'])
    expect(result.text).toBe('check this')
  })

  it('splits concatenated Windows image paths from immediately following prompt text', () => {
    const first = 'C:\\Users\\Milan M\\AppData\\Local\\Temp\\first.png'
    const second = 'C:\\Users\\Milan M\\AppData\\Local\\Temp\\second.png'
    const result = extractUserMessageImagePaths(`${first}${second}what happened?`)

    expect(result.imagePaths).toEqual([first, second])
    expect(result.text).toBe('what happened?')
  })

  it('matches image extensions case-insensitively', () => {
    const result = extractUserMessageImagePaths('"C:\\pics\\A.JPG" and /b/B.JPEG')

    expect(result.imagePaths).toEqual(['C:\\pics\\A.JPG', '/b/B.JPEG'])
    expect(result.text).not.toMatch(/\.JPG|\.JPEG/i)
  })

  it('returns the text unchanged for ordinary sentences without image tokens', () => {
    const sentence = 'was ist auf diesem bild zu sehen?'
    expect(extractUserMessageImagePaths(sentence)).toEqual({ text: sentence, imagePaths: [] })
  })

  it('does not treat paths without an image extension as images', () => {
    const result = extractUserMessageImagePaths('/usr/local/bin/tool did the thing')

    expect(result.imagePaths).toEqual([])
    expect(result.text).toBe('/usr/local/bin/tool did the thing')
  })

  it('keeps a quoted bare filename without a path separator as prose', () => {
    const result = extractUserMessageImagePaths('"screenshot.png" bitte anschauen')

    expect(result.imagePaths).toEqual([])
    expect(result.text).toBe('"screenshot.png" bitte anschauen')
  })

  it('strips a quoted path containing spaces because it has a separator', () => {
    const result = extractUserMessageImagePaths('look at "/home/me/pics/a b.png" now')

    expect(result.imagePaths).toEqual(['/home/me/pics/a b.png'])
    expect(result.text).toBe('look at  now')
  })

  it('dedupes repeated paths while preserving first-seen order', () => {
    const result = extractUserMessageImagePaths('/a/one.png then /b/two.png then /a/one.png again')

    expect(result.imagePaths).toEqual(['/a/one.png', '/b/two.png'])
    expect(result.text).not.toMatch(/\.png/)
  })

  it('returns empty extraction for a message that is only whitespace', () => {
    expect(extractUserMessageImagePaths('   ')).toEqual({ text: '', imagePaths: [] })
  })
})
