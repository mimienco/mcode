import { expect, it, vi } from 'vitest'
import { DesktopCommandError } from './desktop-client'
import { createRuntimeDesktopFilesystemClient } from './runtime-desktop-filesystem-client'

const directoryEntries = [
  { name: 'src', isDirectory: true, isSymlink: false },
  { name: 'README.md', isDirectory: false, isSymlink: false }
]

it('loads the Tauri invoke route lazily and sends exact camelCase arguments', async () => {
  const invoke = vi
    .fn()
    .mockResolvedValueOnce(directoryEntries)
    .mockResolvedValueOnce({ size: 42, isDirectory: false, mtime: 1234 })
    .mockResolvedValueOnce(true)
  const loadTauriInvoke = vi.fn(() => Promise.resolve(invoke))
  const electronFilesystem = {
    readDir: vi.fn(),
    stat: vi.fn(),
    pathExists: vi.fn()
  }
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke,
    getElectronFilesystem: () => electronFilesystem
  })

  expect(loadTauriInvoke).not.toHaveBeenCalled()

  await expect(client.readDir({ dirPath: 'C:\\repo\\src' })).resolves.toEqual(directoryEntries)
  await expect(client.stat({ filePath: 'C:\\repo\\README.md' })).resolves.toEqual({
    size: 42,
    isDirectory: false,
    mtime: 1234
  })
  await expect(client.pathExists({ filePath: 'C:\\repo\\README.md' })).resolves.toBe(true)

  expect(loadTauriInvoke).toHaveBeenCalledOnce()
  expect(invoke.mock.calls).toEqual([
    ['fs_read_dir', { dirPath: 'C:\\repo\\src' }],
    ['fs_stat', { filePath: 'C:\\repo\\README.md' }],
    ['fs_path_exists', { filePath: 'C:\\repo\\README.md' }]
  ])
  expect(electronFilesystem.readDir).not.toHaveBeenCalled()
})

it('falls back to the Electron filesystem without loading Tauri', async () => {
  const electronFilesystem = {
    readDir: vi.fn(() => Promise.resolve(directoryEntries)),
    stat: vi.fn(() => Promise.resolve({ size: 7, isDirectory: true, mtime: 99 })),
    pathExists: vi.fn(() => Promise.resolve(false))
  }
  const loadTauriInvoke = vi.fn()
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => false,
    loadTauriInvoke,
    getElectronFilesystem: () => electronFilesystem
  })

  await expect(client.readDir({ dirPath: '/repo' })).resolves.toEqual(directoryEntries)
  await expect(client.stat({ filePath: '/repo/missing.md' })).resolves.toEqual({
    size: 7,
    isDirectory: true,
    mtime: 99
  })
  await expect(client.pathExists({ filePath: '/repo/missing.md' })).resolves.toBe(false)

  expect(electronFilesystem.readDir).toHaveBeenCalledWith({ dirPath: '/repo' })
  expect(electronFilesystem.stat).toHaveBeenCalledWith({ filePath: '/repo/missing.md' })
  expect(electronFilesystem.pathExists).toHaveBeenCalledWith({ filePath: '/repo/missing.md' })
  expect(loadTauriInvoke).not.toHaveBeenCalled()
})

it('preserves the pinned Electron directory ordering for Unicode and numeric names', async () => {
  const unsorted = [
    { name: 'z', isDirectory: false, isSymlink: false },
    { name: '10', isDirectory: false, isSymlink: false },
    { name: 'ä', isDirectory: false, isSymlink: false },
    { name: '2', isDirectory: false, isSymlink: false },
    { name: 'folder', isDirectory: true, isSymlink: false }
  ]
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.resolve(unsorted))),
    getElectronFilesystem: vi.fn()
  })

  await expect(client.readDir({ dirPath: '/repo' })).resolves.toEqual([
    { name: 'folder', isDirectory: true, isSymlink: false },
    { name: '2', isDirectory: false, isSymlink: false },
    { name: '10', isDirectory: false, isSymlink: false },
    { name: 'ä', isDirectory: false, isSymlink: false },
    { name: 'z', isDirectory: false, isSymlink: false }
  ])
})

it.each([
  ['readDir', 'fs_read_dir', { dirPath: '/repo' }, null],
  [
    'readDir',
    'fs_read_dir',
    { dirPath: '/repo' },
    [{ name: 'src', isDirectory: 'yes', isSymlink: false }]
  ],
  ['stat', 'fs_stat', { filePath: '/repo/a.ts' }, { size: -1, isDirectory: false, mtime: 1 }],
  ['pathExists', 'fs_path_exists', { filePath: '/repo/a.ts' }, 'yes']
] as const)('rejects an invalid %s response', async (method, command, args, response) => {
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.resolve(response))),
    getElectronFilesystem: vi.fn()
  })

  await expect(client[method](args)).rejects.toThrow(`Invalid ${command} response`)
})

it('normalizes a structured Tauri rejection into DesktopCommandError', async () => {
  const rejection = {
    code: 'fs_path_denied',
    context: { operation: 'fs_stat' },
    message: 'path resolves outside the allowed scope',
    userMessage: 'MCode cannot access that path.'
  }
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.reject(rejection))),
    getElectronFilesystem: vi.fn()
  })

  const error = await client.stat({ filePath: '/outside/secret' }).catch((reason) => reason)

  expect(error).toBeInstanceOf(DesktopCommandError)
  expect(error).toMatchObject({
    code: 'fs_path_denied',
    context: { operation: 'fs_stat' },
    message: 'path resolves outside the allowed scope',
    userMessage: 'MCode cannot access that path.'
  })
})

it('preserves Electron rejections for compatibility', async () => {
  const electronError = new Error('ENOENT')
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => false,
    loadTauriInvoke: vi.fn(),
    getElectronFilesystem: () => ({
      readDir: vi.fn(),
      stat: vi.fn(() => Promise.reject(electronError)),
      pathExists: vi.fn()
    })
  })

  await expect(client.stat({ filePath: '/repo/missing' })).rejects.toBe(electronError)
})
