import type { Repo } from './repo-types'

export type LocalRepoCloneResult = Repo | { error: string }

/** Keep expected clone failures as IPC data, then restore the public API's
 * rejected-Promise contract in preload where Electron will not log them as
 * main-process handler failures. */
export function unwrapLocalRepoCloneResult(result: LocalRepoCloneResult): Repo {
  if ('error' in result) {
    throw new Error(result.error)
  }
  return result
}
