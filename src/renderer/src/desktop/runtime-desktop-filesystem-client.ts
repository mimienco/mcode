import { isTauri } from '@tauri-apps/api/core'
import type { DirEntry } from '../../../shared/filesystem-entry-types'
import { sortDirEntries } from '../../../shared/file-name-sort'
import type { DesktopInvoke } from './desktop-client'
import { normalizeDesktopCommandError } from './desktop-client'

export type DesktopFilesystemReadDirArgs = {
  dirPath: string
  connectionId?: string
}

export type DesktopFilesystemPathArgs = {
  filePath: string
  connectionId?: string
}

export type DesktopFilesystemStat = {
  size: number
  isDirectory: boolean
  mtime: number
}

export type RuntimeDesktopFilesystemClient = {
  readDir: (args: DesktopFilesystemReadDirArgs) => Promise<DirEntry[]>
  stat: (args: DesktopFilesystemPathArgs) => Promise<DesktopFilesystemStat>
  pathExists: (args: DesktopFilesystemPathArgs) => Promise<boolean>
}

type ElectronFilesystem = Pick<Window['api']['fs'], 'readDir' | 'stat' | 'pathExists'>

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
    }
  }
}

export const runtimeDesktopFilesystemClient = createRuntimeDesktopFilesystemClient({
  isTauri,
  loadTauriInvoke: async () => (await import('@tauri-apps/api/core')).invoke,
  getElectronFilesystem: () => window.api.fs
})
