import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { isPathInsideAgentRoots } from './session-transcript-path-gate'
import { configureAiVaultSessionSources } from './cached-session-list'

const tempRoots: string[] = []

afterEach(async () => {
  delete process.env.OPENCODE_DB
  delete process.env.XDG_DATA_HOME
  const roots = tempRoots.splice(0)
  await Promise.all(roots.map((root) => rm(root, { recursive: true, force: true })))
})

async function makeTempRoot(prefix: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), prefix))
  tempRoots.push(root)
  return root
}

describe('isPathInsideAgentRoots', () => {
  it('accepts an opencode SQLite candidate whose db sits in the data dir', async () => {
    const dataDir = await makeTempRoot('mcode-gate-opencode-')
    process.env.XDG_DATA_HOME = dataDir
    await mkdir(join(dataDir, 'opencode'), { recursive: true })
    await writeFile(join(dataDir, 'opencode', 'opencode.db'), '')

    expect(
      isPathInsideAgentRoots({
        agent: 'opencode',
        filePath: `${join(dataDir, 'opencode', 'opencode.db')}#ses_1`,
        wslHomeDirs: []
      })
    ).toBe(true)
  })

  it('rejects a synthetic candidate pointing at a database outside every root', async () => {
    const elsewhere = await makeTempRoot('mcode-gate-outside-')
    process.env.XDG_DATA_HOME = await makeTempRoot('mcode-gate-xdg-')

    expect(
      isPathInsideAgentRoots({
        agent: 'opencode',
        filePath: `${join(elsewhere, 'opencode.db')}#ses_1`,
        wslHomeDirs: []
      })
    ).toBe(false)
  })

  it('allows the exact OPENCODE_DB override even when it is out of root', async () => {
    const overrideDb = join(await makeTempRoot('mcode-gate-override-'), 'opencode-custom.db')
    process.env.XDG_DATA_HOME = await makeTempRoot('mcode-gate-xdg2-')
    process.env.OPENCODE_DB = overrideDb

    expect(
      isPathInsideAgentRoots({
        agent: 'opencode',
        filePath: `${overrideDb}#ses_1`,
        wslHomeDirs: []
      })
    ).toBe(true)
  })

  it('accepts an antigravity transcript under a WSL home brain dir', async () => {
    const wslHome = await makeTempRoot('mcode-gate-wsl-home-')
    const brainDir = join(wslHome, '.gemini', 'antigravity-cli', 'brain')
    const sessionId = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee'
    const transcript = join(brainDir, sessionId, '.system_generated', 'logs', 'transcript.jsonl')
    await mkdir(join(brainDir, sessionId, '.system_generated', 'logs'), { recursive: true })
    await writeFile(transcript, '')

    expect(isPathInsideAgentRoots({ agent: 'antigravity', filePath: transcript, wslHomeDirs: [wslHome] })).toBe(
      true
    )
  })

  it('rejects `..` traversal that escapes the matched root textually', () => {
    const wslHome = join(tmpdir(), 'mcode-gate-traversal-home')
    const escaped = join(
      wslHome,
      '.gemini',
      'antigravity-cli',
      'brain',
      '..',
      '..',
      '..',
      '..',
      'ssh-keys.jsonl'
    )

    expect(isPathInsideAgentRoots({ agent: 'antigravity', filePath: escaped, wslHomeDirs: [wslHome] })).toBe(
      false
    )
  })

  it('accepts codex sessions under configured additional codex homes', () => {
    const home = tmpdir()
    configureAiVaultSessionSources({ getAdditionalCodexHomePaths: () => [home] })
    try {
      expect(
        isPathInsideAgentRoots({
          agent: 'codex',
          filePath: join(home, 'sessions', '2026', '08', 'rollout.jsonl'),
          wslHomeDirs: []
        })
      ).toBe(true)
    } finally {
      configureAiVaultSessionSources({})
    }
  })

})
