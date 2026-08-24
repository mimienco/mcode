import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { quarantineCodexStateDb } from './codex-state-db-repair'

const temporaryRoots: string[] = []

afterEach(async () => {
  await Promise.all(
    temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true }))
  )
})

describe('Codex state DB repair', () => {
  it('quarantines the database and SQLite sidecars without touching session history', async () => {
    const home = await mkdtemp(join(tmpdir(), 'mcode-codex-state-db-repair-'))
    temporaryRoots.push(home)
    await mkdir(join(home, 'sessions', '2026', '08', '23'), { recursive: true })
    const sessionPath = join(home, 'sessions', '2026', '08', '23', 'rollout.jsonl')
    const stateDbPath = join(home, 'state_5.sqlite')
    await writeFile(sessionPath, 'session history\n')
    await writeFile(stateDbPath, 'sqlite-main')
    await writeFile(`${stateDbPath}-wal`, 'sqlite-wal')
    await writeFile(`${stateDbPath}-shm`, 'sqlite-shm')

    const recoveryDirectory = await quarantineCodexStateDb(home, stateDbPath)

    expect(recoveryDirectory).toBeTruthy()
    await expect(stat(stateDbPath)).rejects.toMatchObject({ code: 'ENOENT' })
    await expect(readFile(sessionPath, 'utf8')).resolves.toBe('session history\n')
    await expect(readFile(join(recoveryDirectory!, 'state_5.sqlite'), 'utf8')).resolves.toBe(
      'sqlite-main'
    )
    await expect(readFile(join(recoveryDirectory!, 'state_5.sqlite-wal'), 'utf8')).resolves.toBe(
      'sqlite-wal'
    )
    await expect(readFile(join(recoveryDirectory!, 'state_5.sqlite-shm'), 'utf8')).resolves.toBe(
      'sqlite-shm'
    )
  })

  it('rejects a state database outside the selected Codex home', async () => {
    const home = await mkdtemp(join(tmpdir(), 'mcode-codex-state-db-repair-'))
    const outsideRoot = await mkdtemp(join(tmpdir(), 'mcode-codex-state-db-repair-outside-'))
    temporaryRoots.push(home)
    temporaryRoots.push(outsideRoot)
    const outsidePath = join(outsideRoot, 'state_5.sqlite')
    await writeFile(outsidePath, 'must remain')

    await expect(quarantineCodexStateDb(home, outsidePath)).resolves.toBeNull()
    await expect(readFile(outsidePath, 'utf8')).resolves.toBe('must remain')
  })
})
