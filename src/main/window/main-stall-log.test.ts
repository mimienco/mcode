import { mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { appendStallRecord } from './main-stall-log'

let dir: string | null = null

afterEach(() => {
  dir = null
})

describe('appendStallRecord', () => {
  it('appends one ISO-timestamped JSON line per record', () => {
    dir = mkdtempSync(join(tmpdir(), 'mcode-stall-log-'))
    const filePath = join(dir, 'main-stall.log')
    appendStallRecord('main_loop_stall', { driftMs: 3300 }, { filePath })
    appendStallRecord('slow_hidden_output_serialization', { ms: 120, chars: 4096 }, { filePath })
    const lines = readFileSync(filePath, 'utf-8').trim().split('\n')
    expect(lines).toHaveLength(2)
    expect(lines[0]).toMatch(
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}.\d{3}Z main_loop_stall {"driftMs":3300}$/
    )
    expect(lines[1]).toContain('"ms":120')
    expect(lines[1]).toContain('"chars":4096')
  })

  it('swallows write failures instead of throwing into the caller', () => {
    expect(() =>
      appendStallRecord('main_loop_stall', { driftMs: 1 }, { filePath: 'Z:\\no\\such\\dir\\x.log' })
    ).not.toThrow()
  })
})
