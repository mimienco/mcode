import { isTauri } from '@tauri-apps/api/core'
import type { DirEntry } from '../../../shared/filesystem-entry-types'
import { sortDirEntries } from '../../../shared/file-name-sort'
import type { SshMutationExpectation } from '../../../shared/ssh-types'
import type { DesktopInvoke } from './desktop-client'
import { DesktopCommandError, normalizeDesktopCommandError } from './desktop-client'

export type DesktopFilesystemReadDirArgs = {
  dirPath: string
  connectionId?: string
}

export type DesktopFilesystemPathArgs = {
  filePath: string
  connectionId?: string
}

export type DesktopFilesystemReadFileArgs = DesktopFilesystemPathArgs & {
  includeLocalLogMetadata?: boolean
}

export type DesktopFilesystemWriteFileArgs = DesktopFilesystemPathArgs &
  SshMutationExpectation & {
    content: string
  }

export type DesktopFilesystemFileContent = {
  content: string
  isBinary: boolean
  isImage?: boolean
  mimeType?: string
  fileIdentity?: string
}

export type DesktopFilesystemStat = {
  size: number
  isDirectory: boolean
  mtime: number
}

export type RuntimeDesktopFilesystemClient = {
  readDir: (args: DesktopFilesystemReadDirArgs) => Promise<DirEntry[]>
  readFile: (args: DesktopFilesystemReadFileArgs) => Promise<DesktopFilesystemFileContent>
  writeFileAtomic: (args: DesktopFilesystemWriteFileArgs) => Promise<void>
  stat: (args: DesktopFilesystemPathArgs) => Promise<DesktopFilesystemStat>
  pathExists: (args: DesktopFilesystemPathArgs) => Promise<boolean>
  authorizeExternalPath: (targetPath: string) => Promise<void>
  revokeExternalPath: (targetPath: string) => Promise<boolean>
}

type ElectronFilesystem = Pick<
  Window['api']['fs'],
  'readDir' | 'readFile' | 'writeFile' | 'stat' | 'pathExists' | 'authorizeExternalPath'
>

type RuntimeDesktopFilesystemDependencies = {
  isTauri: () => boolean
  loadTauriInvoke: () => Promise<DesktopInvoke>
  getElectronFilesystem: () => ElectronFilesystem
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function isDirEntry(value: unknown): value is DirEntry {
  return (
    isRecord(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    typeof value.isDirectory === 'boolean' &&
    typeof value.isSymlink === 'boolean'
  )
}

function validateReadDir(value: unknown): DirEntry[] {
  if (!Array.isArray(value) || !value.every(isDirEntry)) {
    throw new Error('Invalid fs_read_dir response')
  }
  return value
}

function validateStat(value: unknown): DesktopFilesystemStat {
  if (
    !isRecord(value) ||
    typeof value.size !== 'number' ||
    !Number.isSafeInteger(value.size) ||
    value.size < 0 ||
    typeof value.isDirectory !== 'boolean' ||
    typeof value.mtime !== 'number' ||
    !Number.isFinite(value.mtime)
  ) {
    throw new Error('Invalid fs_stat response')
  }
  return value as DesktopFilesystemStat
}

function validateOptionalString(value: unknown): boolean {
  return value === undefined || typeof value === 'string'
}

function validateReadFile(value: unknown): DesktopFilesystemFileContent {
  if (
    !isRecord(value) ||
    typeof value.content !== 'string' ||
    typeof value.isBinary !== 'boolean' ||
    (value.isImage !== undefined && typeof value.isImage !== 'boolean') ||
    !validateOptionalString(value.mimeType) ||
    !validateOptionalString(value.fileIdentity)
  ) {
    throw new Error('Invalid fs_read_file response')
  }
  return value as DesktopFilesystemFileContent
}

function validatePathExists(value: unknown): boolean {
  if (typeof value !== 'boolean') {
    throw new Error('Invalid fs_path_exists response')
  }
  return value
}

export function createRuntimeDesktopFilesystemClient(
  dependencies: RuntimeDesktopFilesystemDependencies
): RuntimeDesktopFilesystemClient {
  let tauriInvoke: Promise<DesktopInvoke> | undefined
  const loadTauriInvoke = (): Promise<DesktopInvoke> => {
    tauriInvoke ??= dependencies.loadTauriInvoke()
    return tauriInvoke
  }
  const invokeTauri = async (command: string, args: Record<string, unknown>): Promise<unknown> => {
    try {
      const invoke = await loadTauriInvoke()
      return await invoke(command, args)
    } catch (error) {
      throw normalizeDesktopCommandError(error, command)
    }
  }

  return {
    async readDir(args) {
      const response = dependencies.isTauri()
        ? await invokeTauri('fs_read_dir', args)
        : await dependencies.getElectronFilesystem().readDir(args)
      return sortDirEntries(validateReadDir(response))
    },
    async readFile(args) {
      const response = dependencies.isTauri()
        ? await invokeTauri('fs_read_file', args)
        : await dependencies.getElectronFilesystem().readFile(args)
      return validateReadFile(response)
    },
    async writeFileAtomic(args) {
      if (dependencies.isTauri()) {
        const response = await invokeTauri('fs_write_file_atomic', args)
        if (response !== null && response !== undefined) {
          throw new Error('Invalid fs_write_file_atomic response')
        }
        return
      }
      await dependencies.getElectronFilesystem().writeFile(args)
    },
    async stat(args) {
      const response = dependencies.isTauri()
        ? await invokeTauri('fs_stat', args)
        : await dependencies.getElectronFilesystem().stat(args)
      return validateStat(response)
    },
    async pathExists(args) {
      const response = dependencies.isTauri()
        ? await invokeTauri('fs_path_exists', args)
        : await dependencies.getElectronFilesystem().pathExists(args)
      return validatePathExists(response)
    },
    async authorizeExternalPath(targetPath) {
      if (dependencies.isTauri()) {
        let response: unknown
        try {
          response = await invokeTauri('fs_authorize_external_path', { targetPath })
        } catch (error) {
          // Tauri claims only paths staged by a native producer. A path can
          // already belong to a trusted root, so an unstaged claim is a no-op;
          // the following filesystem operation still enforces the real scope.
          if (error instanceof DesktopCommandError && error.code === 'fs_path_denied') {
            return
          }
          throw error
        }
        if (response !== null && response !== undefined) {
          throw new Error('Invalid fs_authorize_external_path response')
        }
        return
      }
      await dependencies.getElectronFilesystem().authorizeExternalPath({ targetPath })
    },
    async revokeExternalPath(targetPath) {
      if (!dependencies.isTauri()) {
        return false
      }
      const response = await invokeTauri('fs_revoke_external_path', { targetPath })
      if (typeof response !== 'boolean') {
        throw new Error('Invalid fs_revoke_external_path response')
      }
      return response
    }
  }
}

export const runtimeDesktopFilesystemClient = createRuntimeDesktopFilesystemClient({
  isTauri,
  loadTauriInvoke: async () => (await import('@tauri-apps/api/core')).invoke,
  getElectronFilesystem: () => window.api.fs
})
