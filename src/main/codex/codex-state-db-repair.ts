import { randomUUID } from 'node:crypto'
import { mkdir, rename, rm, stat } from 'node:fs/promises'
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'

const STATE_DB_FILE_PATTERN = /^state_\d+\.sqlite$/
const STATE_DB_RECOVERY_DIRECTORY = '.mcode/state-db-recovery'

/**
 * Moves a stuck Codex state database and its SQLite sidecars out of the way.
 *
 * The move is deliberately recoverable: rollout/session files are never touched,
 * and the original DB can be restored from the returned directory if a rebuild
 * fails. The caller must hold the per-home recovery lock before invoking this.
 */
export async function quarantineCodexStateDb(
  codexHomePath: string,
  stateDbPath: string
): Promise<string | null> {
  const stateDbName = basename(stateDbPath)
  if (!STATE_DB_FILE_PATTERN.test(stateDbName)) {
    return null
  }

  const homePath = resolve(codexHomePath)
  const candidatePath = resolve(stateDbPath)
  const relativeCandidate = relative(homePath, candidatePath)
  if (
    relativeCandidate === '' ||
    relativeCandidate === '..' ||
    relativeCandidate.startsWith(`..${sep}`) ||
    isAbsolute(relativeCandidate) ||
    dirname(relativeCandidate) !== '.'
  ) {
    return null
  }

  try {
    await stat(candidatePath)
  } catch {
    return null
  }

  const recoveryDirectory = join(
    homePath,
    STATE_DB_RECOVERY_DIRECTORY,
    `${stateDbName}-${Date.now()}-${randomUUID()}`
  )
  await mkdir(recoveryDirectory, { recursive: true })

  const sourcePaths = [candidatePath, `${candidatePath}-wal`, `${candidatePath}-shm`]
  const movedPaths: { source: string; target: string }[] = []
  try {
    for (const sourcePath of sourcePaths) {
      try {
        await stat(sourcePath)
      } catch (error) {
        if ((error as NodeJS.ErrnoException | null)?.code === 'ENOENT') {
          continue
        }
        throw error
      }
      const targetPath = join(recoveryDirectory, basename(sourcePath))
      await rename(sourcePath, targetPath)
      movedPaths.push({ source: sourcePath, target: targetPath })
    }
    return recoveryDirectory
  } catch (error) {
    for (const moved of movedPaths.toReversed()) {
      try {
        await rename(moved.target, moved.source)
      } catch (rollbackError) {
        console.warn(
          '[codex-state-db-repair] Failed to roll back quarantined state DB:',
          rollbackError
        )
      }
    }
    await rm(recoveryDirectory, { recursive: true, force: true }).catch(() => {})
    throw error
  }
}
