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
    .mockResolvedValueOnce({ content: 'hello', isBinary: false })
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce({ size: 42, isDirectory: false, mtime: 1234 })
    .mockResolvedValueOnce(true)
    .mockResolvedValueOnce({ authorization: 'nativeDrop' })
    .mockResolvedValueOnce(true)
  const loadTauriInvoke = vi.fn(() => Promise.resolve(invoke))
  const electronFilesystem = {
    readDir: vi.fn(),
    readFile: vi.fn(),
    writeFile: vi.fn(),
    stat: vi.fn(),
    pathExists: vi.fn(),
    authorizeExternalPath: vi.fn()
  }
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke,
    getElectronFilesystem: () => electronFilesystem
  })

  expect(loadTauriInvoke).not.toHaveBeenCalled()

  await expect(client.readDir({ dirPath: 'C:\\repo\\src' })).resolves.toEqual(directoryEntries)
  await expect(client.readFile({ filePath: 'C:\\repo\\README.md' })).resolves.toEqual({
    content: 'hello',
    isBinary: false
  })
  await expect(
    client.writeFileAtomic({ filePath: 'C:\\repo\\README.md', content: 'changed' })
  ).resolves.toBeUndefined()
  await expect(client.stat({ filePath: 'C:\\repo\\README.md' })).resolves.toEqual({
    size: 42,
    isDirectory: false,
    mtime: 1234
  })
  await expect(client.pathExists({ filePath: 'C:\\repo\\README.md' })).resolves.toBe(true)
  await expect(client.authorizeExternalPath('C:\\outside\\report.md')).resolves.toBeUndefined()
  await expect(client.revokeExternalPath('C:\\outside\\report.md')).resolves.toBe(true)

  expect(loadTauriInvoke).toHaveBeenCalledOnce()
  expect(invoke.mock.calls).toEqual([
    ['fs_read_dir', { dirPath: 'C:\\repo\\src' }],
    ['fs_read_file', { filePath: 'C:\\repo\\README.md' }],
    ['fs_write_file_atomic', { filePath: 'C:\\repo\\README.md', content: 'changed' }],
    ['fs_stat', { filePath: 'C:\\repo\\README.md' }],
    ['fs_path_exists', { filePath: 'C:\\repo\\README.md' }],
    ['fs_authorize_external_path', { targetPath: 'C:\\outside\\report.md' }],
    ['fs_revoke_external_path', { targetPath: 'C:\\outside\\report.md' }]
  ])
  expect(electronFilesystem.readDir).not.toHaveBeenCalled()
})

it('falls back to the Electron filesystem without loading Tauri', async () => {
  const electronFilesystem = {
    readDir: vi.fn(() => Promise.resolve(directoryEntries)),
    readFile: vi.fn(() => Promise.resolve({ content: 'hello', isBinary: false })),
    writeFile: vi.fn(() => Promise.resolve()),
    stat: vi.fn(() => Promise.resolve({ size: 7, isDirectory: true, mtime: 99 })),
    pathExists: vi.fn(() => Promise.resolve(false)),
    authorizeExternalPath: vi.fn(() => Promise.resolve())
  }
  const loadTauriInvoke = vi.fn()
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => false,
    loadTauriInvoke,
    getElectronFilesystem: () => electronFilesystem
  })

  await expect(client.readDir({ dirPath: '/repo' })).resolves.toEqual(directoryEntries)
  await expect(client.readFile({ filePath: '/repo/readme.md' })).resolves.toEqual({
    content: 'hello',
    isBinary: false
  })
  await expect(
    client.writeFileAtomic({ filePath: '/repo/readme.md', content: 'changed' })
  ).resolves.toBeUndefined()
  await expect(client.stat({ filePath: '/repo/missing.md' })).resolves.toEqual({
    size: 7,
    isDirectory: true,
    mtime: 99
  })
  await expect(client.pathExists({ filePath: '/repo/missing.md' })).resolves.toBe(false)
  await expect(client.authorizeExternalPath('/outside/report.md')).resolves.toBeUndefined()
  await expect(client.revokeExternalPath('/outside/report.md')).resolves.toBe(false)

  expect(electronFilesystem.readDir).toHaveBeenCalledWith({ dirPath: '/repo' })
  expect(electronFilesystem.readFile).toHaveBeenCalledWith({ filePath: '/repo/readme.md' })
  expect(electronFilesystem.writeFile).toHaveBeenCalledWith({
    filePath: '/repo/readme.md',
    content: 'changed'
  })
  expect(electronFilesystem.stat).toHaveBeenCalledWith({ filePath: '/repo/missing.md' })
  expect(electronFilesystem.pathExists).toHaveBeenCalledWith({ filePath: '/repo/missing.md' })
  expect(electronFilesystem.authorizeExternalPath).toHaveBeenCalledWith({
    targetPath: '/outside/report.md'
  })
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

it('rejects invalid filesystem responses', async () => {
  const clientFor = (response: unknown) =>
    createRuntimeDesktopFilesystemClient({
      isTauri: () => true,
      loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.resolve(response))),
      getElectronFilesystem: vi.fn()
    })

  await expect(clientFor(null).readDir({ dirPath: '/repo' })).rejects.toThrow(
    'Invalid fs_read_dir response'
  )
  await expect(
    clientFor([{ name: 'src', isDirectory: 'yes', isSymlink: false }]).readDir({
      dirPath: '/repo'
    })
  ).rejects.toThrow('Invalid fs_read_dir response')
  await expect(
    clientFor({ content: 42, isBinary: false }).readFile({ filePath: '/repo/a.ts' })
  ).rejects.toThrow('Invalid fs_read_file response')
  await expect(
    clientFor({ size: -1, isDirectory: false, mtime: 1 }).stat({ filePath: '/repo/a.ts' })
  ).rejects.toThrow('Invalid fs_stat response')
  await expect(clientFor('yes').pathExists({ filePath: '/repo/a.ts' })).rejects.toThrow(
    'Invalid fs_path_exists response'
  )
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

it('forwards local log metadata requests and validates the snapshot identity', async () => {
  const invoke = vi.fn(() =>
    Promise.resolve({ content: 'first', isBinary: false, fileIdentity: '1:2:3' })
  )
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(invoke),
    getElectronFilesystem: vi.fn()
  })

  const result = await client.readFile({
    filePath: '/repo/live.log',
    includeLocalLogMetadata: true
  })

  expect(invoke).toHaveBeenCalledWith('fs_read_file', {
    filePath: '/repo/live.log',
    includeLocalLogMetadata: true
  })
  expect(result).toEqual({
    content: 'first',
    isBinary: false,
    fileIdentity: '1:2:3'
  })
})

it('preserves direct SSH mutation expectations on the Electron write fallback', async () => {
  const writeFile = vi.fn(() => Promise.resolve())
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => false,
    loadTauriInvoke: vi.fn(),
    getElectronFilesystem: () => ({
      readDir: vi.fn(),
      readFile: vi.fn(),
      writeFile,
      stat: vi.fn(),
      pathExists: vi.fn(),
      authorizeExternalPath: vi.fn()
    })
  })
  const request = {
    filePath: '/repo/.mcp.json',
    content: '{}',
    connectionId: 'server',
    expectedExecutionHostId: 'ssh:server' as const,
    expectedSshTargetId: 'server',
    expectedSshConnectionGeneration: 7
  }

  await expect(client.writeFileAtomic(request)).resolves.toBeUndefined()
  expect(writeFile).toHaveBeenCalledWith(request)
})

it('rejects a non-void atomic write response', async () => {
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.resolve({ ok: true }))),
    getElectronFilesystem: vi.fn()
  })

  await expect(client.writeFileAtomic({ filePath: '/repo/a.ts', content: 'a' })).rejects.toThrow(
    'Invalid fs_write_file_atomic response'
  )
})

it('preserves Electron rejections for compatibility', async () => {
  const electronError = new Error('ENOENT')
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => false,
    loadTauriInvoke: vi.fn(),
    getElectronFilesystem: () => ({
      readDir: vi.fn(),
      readFile: vi.fn(),
      writeFile: vi.fn(),
      stat: vi.fn(() => Promise.reject(electronError)),
      pathExists: vi.fn(),
      authorizeExternalPath: vi.fn()
    })
  })

  await expect(client.stat({ filePath: '/repo/missing' })).rejects.toBe(electronError)
})

it('validates external grant responses', async () => {
  const clientFor = (response: unknown) =>
    createRuntimeDesktopFilesystemClient({
      isTauri: () => true,
      loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.resolve(response))),
      getElectronFilesystem: vi.fn()
    })

  await expect(clientFor(true).authorizeExternalPath('/outside/report.md')).rejects.toThrow(
    'Invalid fs_authorize_external_path response'
  )
  await expect(clientFor(null).revokeExternalPath('/outside/report.md')).rejects.toThrow(
    'Invalid fs_revoke_external_path response'
  )
})

it('accepts an explicit trusted-root authorization result', async () => {
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () =>
      Promise.resolve(vi.fn(() => Promise.resolve({ authorization: 'trustedRoot' }))),
    getElectronFilesystem: vi.fn()
  })

  await expect(client.authorizeExternalPath('/repo/report.md')).resolves.toBeUndefined()
})

it('rejects an unstaged Tauri claim instead of silently widening the renderer contract', async () => {
  const rejection = {
    code: 'fs_path_denied',
    context: { operation: 'fs_authorize_external_path' },
    message: 'path cannot be authorized',
    userMessage: 'MCode cannot access that path.'
  }
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.reject(rejection))),
    getElectronFilesystem: vi.fn()
  })

  const error = await client.authorizeExternalPath('/outside/report.md').catch((reason) => reason)

  expect(error).toBeInstanceOf(DesktopCommandError)
  expect(error).toMatchObject(rejection)
})

it('normalizes non-denial external grant Tauri rejections', async () => {
  const rejection = {
    code: 'fs_io',
    context: { operation: 'fs_authorize_external_path' },
    message: 'claim failed',
    userMessage: 'MCode cannot access that path.'
  }
  const client = createRuntimeDesktopFilesystemClient({
    isTauri: () => true,
    loadTauriInvoke: () => Promise.resolve(vi.fn(() => Promise.reject(rejection))),
    getElectronFilesystem: vi.fn()
  })

  const error = await client.authorizeExternalPath('/outside/secret').catch((reason) => reason)

  expect(error).toBeInstanceOf(DesktopCommandError)
  expect(error).toMatchObject(rejection)
})
