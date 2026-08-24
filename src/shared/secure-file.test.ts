import { execFile, execFileSync } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  __getSecureFileHardeningCacheStateForTests,
  __resetSecureFileHardenedPathsForTests,
  __resetSecureFileWindowsUserSidForTests,
  hardenExistingSecureFile,
  hardenSecurePath,
  writeSecureFile
} from './secure-file'

const posixModeIt = process.platform === 'win32' ? it.skip : it

vi.mock('child_process', () => ({
  execFileSync: vi.fn(),
  execFile: vi.fn()
}))

describe('hardenSecurePath', () => {
  const originalSystemRoot = process.env.SystemRoot
  const originalWindir = process.env.WINDIR
  const originalPlatform = Object.getOwnPropertyDescriptor(process, 'platform')
  const tempDirs: string[] = []

  beforeEach(() => {
    process.env.SystemRoot = 'C:\\Windows'
    delete process.env.WINDIR
    __resetSecureFileWindowsUserSidForTests()
    __resetSecureFileHardenedPathsForTests()
    vi.mocked(execFileSync).mockReset()
    vi.mocked(execFile).mockReset()
    // execFileSync handles whoami.exe (SID lookup) and the SYNCHRONOUS icacls file-ACL
    // path used by writeSecureFile. The directory + read-path re-harden use async execFile.
    vi.mocked(execFileSync).mockImplementation((file) => {
      if (file === 'C:\\Windows\\System32\\whoami.exe') {
        return '"USER","S-1-5-21-1000"'
      }
      // Synchronous icacls ACL apply succeeds (returns empty stdout).
      return ''
    })
    // Directory + read-path icacls is called asynchronously; simulate immediate success
    vi.mocked(execFile).mockImplementation((_file, _args, _opts, callback) => {
      if (typeof callback === 'function') {
        callback(null, '', '')
      }
      return {} as ReturnType<typeof execFile>
    })
  })

  afterEach(() => {
    if (originalSystemRoot === undefined) {
      delete process.env.SystemRoot
    } else {
      process.env.SystemRoot = originalSystemRoot
    }
    if (originalWindir === undefined) {
      delete process.env.WINDIR
    } else {
      process.env.WINDIR = originalWindir
    }
    __resetSecureFileWindowsUserSidForTests()
    __resetSecureFileHardenedPathsForTests()
    if (originalPlatform) {
      Object.defineProperty(process, 'platform', originalPlatform)
    }
    for (const dir of tempDirs.splice(0)) {
      rmSync(dir, { recursive: true, force: true })
    }
  })

  it('rewrites Windows ACLs through the system icacls binary', () => {
    hardenSecurePath('C:\\Users\\me\\.mcode\\secret.json', {
      isDirectory: false,
      platform: 'win32'
    })

    // whoami.exe called synchronously to obtain SID
    expect(execFileSync).toHaveBeenNthCalledWith(
      1,
      'C:\\Windows\\System32\\whoami.exe',
      ['/user', '/fo', 'csv', '/nh'],
      expect.objectContaining({ encoding: 'utf-8' })
    )
    // icacls called asynchronously
    const [icaclsFile, icaclsArgs, icaclsOptions] = vi.mocked(execFile).mock.calls[0]!
    expect(icaclsFile).toBe('C:\\Windows\\System32\\icacls.exe')
    expect(icaclsArgs).toEqual([
      'C:\\Users\\me\\.mcode\\secret.json',
      '/inheritance:r',
      '/grant:r',
      '*S-1-5-21-1000:F',
      '*S-1-5-18:F',
      '*S-1-5-32-544:F'
    ])
    expect(icaclsOptions).toEqual(expect.objectContaining({ windowsHide: true, timeout: 5000 }))
  })

  it('adds inheritable rules when hardening a Windows directory', () => {
    hardenSecurePath('C:\\Users\\me\\.mcode', { isDirectory: true, platform: 'win32' })

    const icaclsArgs = vi.mocked(execFile).mock.calls[0]![1] as string[]
    expect(icaclsArgs.at(-3)).toContain('(OI)(CI)F')
  })

  it('keeps Windows hardening best-effort when ACL rewriting fails', () => {
    // Simulate async icacls failure — the callback receives an error
    vi.mocked(execFile).mockImplementationOnce((_file, _args, _opts, callback) => {
      if (typeof callback === 'function') {
        callback(new Error('access denied'), '', '')
      }
      return {} as ReturnType<typeof execFile>
    })

    expect(() =>
      hardenSecurePath('C:\\Users\\me\\.mcode\\secret.json', {
        isDirectory: false,
        platform: 'win32'
      })
    ).not.toThrow()
  })

  it('caches successful existing-file hardening within a process', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')
    writeFileSync(targetPath, '{}')

    hardenExistingSecureFile(targetPath)
    hardenExistingSecureFile(targetPath)

    // dir hardened once (SYNC, confirmed — children inherit its restricted ACEs),
    // file hardened once (metadata-cached async)
    expect(getSyncIcaclsCalls().map(getIcaclsTarget)).toEqual([userDataPath])
    expect(getIcaclsCalls().map(getIcaclsTarget)).toEqual([targetPath])
  })

  it('LRU-evicts Windows file hardening entries and safely re-hardens an evicted path', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    __resetSecureFileHardenedPathsForTests({
      maxEntries: 2,
      maxKeyBytes: 4096,
      maxTotalKeyBytes: 8192
    })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const paths = ['first.json', 'second.json', 'third.json'].map((name) =>
      join(userDataPath, name)
    )
    for (const path of paths) {
      writeFileSync(path, '{}')
      hardenExistingSecureFile(path)
    }

    hardenExistingSecureFile(paths[0]!)

    const fileTargets = getIcaclsCalls()
      .map(getIcaclsTarget)
      .filter((path) => paths.includes(path))
    expect(fileTargets).toEqual([...paths, paths[0]])
    expect(__getSecureFileHardeningCacheStateForTests().paths).toMatchObject({
      entries: 2
    })
  })

  it('LRU-evicts Windows directory hardening entries instead of retaining every path', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    __resetSecureFileHardenedPathsForTests({
      maxEntries: 2,
      maxKeyBytes: 4096,
      maxTotalKeyBytes: 8192
    })
    const root = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(root)
    const directories = ['first', 'second', 'third'].map((name) => join(root, name))
    const files = directories.map((dir) => {
      mkdirSync(dir)
      const file = join(dir, 'secret.json')
      writeFileSync(file, '{}')
      return file
    })
    for (const file of files) {
      hardenExistingSecureFile(file)
    }

    hardenExistingSecureFile(files[0]!)

    // Directories harden via the SYNCHRONOUS path (confirmed before caching); the LRU
    // still evicts them, and the re-hardened dir goes through sync again.
    const directoryTargets = getSyncIcaclsCalls()
      .map(getIcaclsTarget)
      .filter((path) => directories.includes(path))
    expect(directoryTargets).toEqual([...directories, directories[0]])
    expect(__getSecureFileHardeningCacheStateForTests().directories).toMatchObject({
      entries: 2
    })
  })

  it('re-hardens an existing file when its metadata changes after caching', async () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')
    writeFileSync(targetPath, '{}')

    hardenExistingSecureFile(targetPath)
    await waitForFileTimestampTick()
    writeFileSync(targetPath, '{"changed":true}')
    hardenExistingSecureFile(targetPath)

    // call 1: dir (sync) + file (async). call 2: dir skipped (cached), file re-hardened (new mtime)
    expect(getSyncIcaclsCalls().map(getIcaclsTarget)).toEqual([userDataPath])
    expect(getIcaclsCalls().map(getIcaclsTarget)).toEqual([
      targetPath,
      targetPath
    ])
  })

  it('writes into a confirmed-hardened directory skip per-file icacls entirely', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')

    writeSecureFile(targetPath, 'first')
    writeSecureFile(targetPath, 'second')

    // The DIRECTORY is hardened once, synchronously confirmed; files created inside
    // inherit its restricted ACEs at create time, so NEITHER write spawns icacls for
    // the tmpFile or the published target — inheritance already restricts them.
    expect(getSyncIcaclsCalls().map(getIcaclsTarget)).toEqual([userDataPath])
    expect(getIcaclsCalls()).toHaveLength(0)

    // Both published files must exist and hold the written contents.
    expect(readFileSync(targetPath, 'utf8')).toBe('second')
  })

  it('falls back to synchronous per-file hardening when the directory harden fails', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')

    // Simulate icacls rejecting DIRECTORY ACLs (the (OI)(CI) invocation) while file
    // grants still succeed — the write path must keep the old per-file guarantee.
    vi.mocked(execFileSync).mockImplementation((file, args) => {
      if (file === 'C:\\Windows\\System32\\whoami.exe') {
        return '"USER","S-1-5-21-1000"'
      }
      const argv = args as string[]
      if (argv.some((a) => String(a).includes('(OI)(CI)'))) {
        throw new Error('access denied')
      }
      return ''
    })

    writeSecureFile(targetPath, 'contents')

    // Fallback engaged: both the tmpFile and the published target were hardened
    // synchronously before/after the rename, and the failed dir stayed uncached.
    const syncTargets = getSyncIcaclsCalls().map(getIcaclsTarget)
    expect(syncTargets[0]).toBe(userDataPath)
    expect(syncTargets.filter((entry) => entry === targetPath)).toHaveLength(1)
    expect(syncTargets.some((entry) => entry.endsWith('.tmp'))).toBe(true)
    expect(readFileSync(targetPath, 'utf8')).toBe('contents')
  })

  // Regression test: #4901 — env-store reads at ~2×/s caused a PowerShell storm because the
  // parent directory mtime churned (every secure write updates it), so the mtime-keyed cache
  // never matched. Directories must be path-cached for the process lifetime.
  it('does not re-harden the parent directory when its mtime changes between reads', async () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')
    writeFileSync(targetPath, '{}')

    // Simulate the env-store read loop: hardenExistingSecureFile called many times while
    // another part of MCode writes to the same directory (changing its mtime).
    hardenExistingSecureFile(targetPath)
    await waitForFileTimestampTick()
    // Simulate a write to another file in the same dir (changes dir mtime)
    writeFileSync(join(userDataPath, 'other.json'), '{}')
    hardenExistingSecureFile(targetPath)
    hardenExistingSecureFile(targetPath)

    // The parent directory must be hardened exactly ONCE despite its mtime changing
    // (now via the synchronous confirmed path)
    const dirCalls = getSyncIcaclsCalls().filter(
      (call) => getIcaclsTarget(call) === userDataPath
    )
    expect(dirCalls).toHaveLength(1)
  })

  it('does not re-harden an unchanged file on repeated reads', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')
    writeFileSync(targetPath, '{}')

    hardenExistingSecureFile(targetPath)
    hardenExistingSecureFile(targetPath)
    hardenExistingSecureFile(targetPath)

    const fileCalls = getIcaclsCalls().filter(
      (call) => getIcaclsTarget(call) === targetPath
    )
    expect(fileCalls).toHaveLength(1)
  })

  it('applies the read-path ACL asynchronously without blocking (async execFile)', () => {
    hardenSecurePath('C:\\Users\\me\\.mcode\\secret.json', {
      isDirectory: false,
      platform: 'win32'
    })

    // The default (read/dir) path must launch icacls via execFile (async), never sync.
    expect(getSyncIcaclsCalls()).toHaveLength(0)
    expect(getIcaclsCalls()).toHaveLength(1)
  })

  // Security regression guard (#5006 review finding): writeSecureFile must never publish
  // the credential readable under broader ACLs. Two valid designs: (a) a synchronous
  // per-file ACL before rename, or (b) a CONFIRMED-restricted parent directory whose
  // inheritable ACEs apply at file-creation time. Design (b) is the steady state; this
  // test pins it — the dir hardens synchronously once, and no async file-ACL window exists.
  it('publishes credentials via confirmed dir inheritance with no async ACL window', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')

    writeSecureFile(targetPath, 'contents')

    // Directory: synchronous + confirmed, exactly once.
    expect(getSyncIcaclsCalls().map(getIcaclsTarget)).toEqual([userDataPath])
    // No async icacls anywhere in the write path — nothing is left best-effort.
    expect(getIcaclsCalls()).toHaveLength(0)
  })

  // Nit #1 (review): the synchronous file path must cache as hardened ONLY on confirmed
  // success, so a failed ACL apply is retried on the next write instead of being silently
  // trusted.
  it('retries the credential-file ACL on the next write when the sync apply fails', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')

    // First write: the synchronous icacls ACL apply throws for every icacls call.
    vi.mocked(execFileSync).mockImplementation((file) => {
      if (file === 'C:\\Windows\\System32\\whoami.exe') {
        return '"USER","S-1-5-21-1000"'
      }
      throw new Error('access denied')
    })
    expect(() => writeSecureFile(targetPath, 'first')).not.toThrow()
    const firstWriteTargetCalls = getSyncIcaclsCalls()
      .map(getIcaclsTarget)
      .filter((entry) => entry === targetPath)
    expect(firstWriteTargetCalls).toHaveLength(1)

    // Second write: ACL apply now succeeds. The dir hardens (confirmed), so the target
    // file needs no direct ACL — it inherits the restricted ACEs at creation.
    vi.mocked(execFileSync).mockImplementation((file) => {
      if (file === 'C:\\Windows\\System32\\whoami.exe') {
        return '"USER","S-1-5-21-1000"'
      }
      return ''
    })
    writeSecureFile(targetPath, 'second')
    const allTargetCalls = getSyncIcaclsCalls()
      .map(getIcaclsTarget)
      .filter((entry) => entry === targetPath)
    expect(allTargetCalls).toHaveLength(1)
    // The recovered dir harden ran exactly once more (the successful confirmation).
    const dirCalls = getSyncIcaclsCalls()
      .map(getIcaclsTarget)
      .filter((entry) => entry === userDataPath)
    expect(dirCalls).toHaveLength(2)
  })

  // Nit #2 (review) / hardening: the process-lifetime directory cache hardens a directory
  // exactly once even when its mtime churns across many writes (the #4901 storm condition,
  // exercised through the write path rather than the read path).
  it('hardens the directory exactly once across many writes despite mtime churn', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'win32' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)

    for (let i = 0; i < 5; i++) {
      // Each write changes the directory's mtime (a new file lands in it).
      writeSecureFile(join(userDataPath, `secret-${i}.json`), `contents-${i}`)
    }

    const dirCalls = getSyncIcaclsCalls().filter(
      (call) => getIcaclsTarget(call) === userDataPath
    )
    expect(dirCalls).toHaveLength(1)
  })

  // win32-only guard: on non-win32 platforms no icacls is ever spawned (sync or async);
  // POSIX hardening uses chmodSync only.
  it('never spawns icacls on non-win32 platforms', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'linux' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')

    writeSecureFile(targetPath, 'contents')
    hardenExistingSecureFile(targetPath)

    expect(getIcaclsCalls()).toHaveLength(0)
    expect(getSyncIcaclsCalls()).toHaveLength(0)
  })

  posixModeIt('re-hardens a POSIX directory when its metadata changes after caching', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'linux' })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const targetPath = join(userDataPath, 'secret.json')
    writeFileSync(targetPath, '{}')

    hardenExistingSecureFile(targetPath)
    expect(statMode(userDataPath)).toBe(0o700)

    chmodSync(userDataPath, 0o755)
    hardenExistingSecureFile(targetPath)

    expect(statMode(userDataPath)).toBe(0o700)
  })

  posixModeIt('LRU-bounds POSIX hardening entries while keeping recent paths cached', () => {
    Object.defineProperty(process, 'platform', { configurable: true, value: 'linux' })
    __resetSecureFileHardenedPathsForTests({
      maxEntries: 2,
      maxKeyBytes: 4096,
      maxTotalKeyBytes: 8192
    })
    const userDataPath = mkdtempSync(join(tmpdir(), 'mcode-secure-file-'))
    tempDirs.push(userDataPath)
    const firstPath = join(userDataPath, 'first.json')
    const secondPath = join(userDataPath, 'second.json')
    writeFileSync(firstPath, '{}')
    writeFileSync(secondPath, '{}')

    hardenExistingSecureFile(firstPath)
    hardenExistingSecureFile(secondPath)
    expect(__getSecureFileHardeningCacheStateForTests().paths.paths).toEqual([
      userDataPath,
      secondPath
    ])

    hardenExistingSecureFile(firstPath)
    expect(__getSecureFileHardeningCacheStateForTests().paths.paths).toEqual([
      userDataPath,
      firstPath
    ])
  })
})

const ICACLS_SUFFIX = 'icacls.exe'

// Async icacls calls (directory hardening + read-path file re-harden).
function getIcaclsCalls(): unknown[][] {
  return vi.mocked(execFile).mock.calls.filter(([file]) => String(file).endsWith(ICACLS_SUFFIX))
}

// Synchronous icacls calls (credential-file ACL on the write path).
function getSyncIcaclsCalls(): unknown[][] {
  return vi
    .mocked(execFileSync)
    .mock.calls.filter(([file]) => String(file).endsWith(ICACLS_SUFFIX))
}

function getIcaclsTarget(call: unknown[]): string {
  return (call[1] as string[])[0]!
}

async function waitForFileTimestampTick(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 20))
}

function statMode(path: string): number {
  return statSync(path).mode & 0o777
}
