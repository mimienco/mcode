import assert from 'node:assert/strict'
import { describe, expect, it as test, vi } from 'vitest'
import {
  FS_CHANGED_EVENT,
  createRuntimeFsClient,
  createTauriFsClient,
  type FsApi,
  type FsChangedPayload
} from './fs-client'
import { DesktopCommandError } from './desktop-client'

function recordedInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  const invoke = async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args })
    const handler = handlers[command]
    if (!handler) {
      throw new Error(`unexpected command: ${command}`)
    }
    return handler(args ?? {})
  }
  return { calls, invoke }
}

function failListener(): (payload: FsChangedPayload) => void {
  return () => {
    throw new Error('no fs change event should reach this listener')
  }
}

test('typed client maps one method onto each fs command with camelCase args', async () => {
  const { calls, invoke } = recordedInvoke({
    fs_read_dir: () => [{ name: 'docs', isDirectory: true, isSymlink: false }],
    fs_read_file: () => ({ content: 'hello', isBinary: false }),
    fs_write_file: () => null,
    fs_path_exists: () => true,
    fs_stat: () => ({ size: 3, isDirectory: false, mtime: 12 }),
    fs_create_dir: () => null,
    fs_create_file: () => null,
    fs_copy_path: () => null,
    fs_rename_path: () => null,
    fs_delete_path: () => null,
    fs_write_file_atomic: () => null,
    fs_search: () => ({ matches: [], truncated: false }),
    fs_search_replace: () => ({ outcomes: {}, cancelled: false }),
    fs_cancel_list_files: () => null,
    fs_cancel_search_replace: () => null,
    fs_watch_worktree: () => null,
    fs_unwatch_worktree: () => null
  })
  const client = createTauriFsClient(invoke, async () => () => undefined)

  const entry = await client.readDir({ dirPath: 'D:/repo' })
  assert.deepEqual(entry, [{ name: 'docs', isDirectory: true, isSymlink: false }])
  assert.deepEqual(await client.readFile({ filePath: 'D:/repo/a.md' }), {
    content: 'hello',
    isBinary: false
  })
  await client.writeFile({ filePath: 'D:/repo/a.md', content: 'x' })
  assert.equal(await client.pathExists({ filePath: 'D:/repo' }), true)
  assert.deepEqual(await client.stat({ filePath: 'D:/repo/a.md' }), {
    size: 3,
    isDirectory: false,
    mtime: 12
  })
  await client.createDir({ dirPath: 'D:/repo/new' })
  await client.createFile({ filePath: 'D:/repo/new/b.txt' })
  await client.copyPath({ sourcePath: 'D:/repo/a', destinationPath: 'D:/repo/b' })
  await client.renamePath({ oldPath: 'D:/repo/a', newPath: 'D:/repo/b' })
  await client.deletePath({ targetPath: 'D:/repo/b' })
  await client.writeFileAtomic({ filePath: 'D:/repo/a.md', content: 'x' })
  const search = await client.search({ rootPath: 'D:/repo', query: 'q', maxResults: 10 })
  assert.deepEqual(search, { matches: [], truncated: false })
  const replace = await client.searchReplace({ rootPath: 'D:/repo', query: 'a', replacement: 'b' })
  assert.deepEqual(replace, { outcomes: {}, cancelled: false })
  await client.cancelListFiles({ requestToken: 't1' })
  await client.cancelSearchReplace({ requestId: 'r1' })
  await client.watchWorktree({ worktreePath: 'D:/repo' })
  await client.unwatchWorktree({ worktreePath: 'D:/repo' })

  assert.deepEqual(
    calls.map(({ command }) => command),
    [
      'fs_read_dir',
      'fs_read_file',
      'fs_write_file',
      'fs_path_exists',
      'fs_stat',
      'fs_create_dir',
      'fs_create_file',
      'fs_copy_path',
      'fs_rename_path',
      'fs_delete_path',
      'fs_write_file_atomic',
      'fs_search',
      'fs_search_replace',
      'fs_cancel_list_files',
      'fs_cancel_search_replace',
      'fs_watch_worktree',
      'fs_unwatch_worktree'
    ]
  )
  assert.deepEqual(calls[0]?.args, { dirPath: 'D:/repo' })
  assert.deepEqual(calls[8]?.args, { oldPath: 'D:/repo/a', newPath: 'D:/repo/b' })
  assert.deepEqual(calls[9]?.args, { targetPath: 'D:/repo/b' })
  assert.deepEqual(calls[10]?.args, { filePath: 'D:/repo/a.md', content: 'x' })
  assert.deepEqual(calls[11]?.args, {
    rootPath: 'D:/repo',
    query: 'q',
    maxResults: 10,
    requestToken: undefined
  })
})

test('invoke failures normalize onto DesktopCommandError with the native code', async () => {
  const client = createTauriFsClient(
    async () => {
      throw {
        code: 'fs_path_denied',
        context: { operation: 'fs_read_dir', requestId: null, intent: null },
        message: 'Access denied: path resolves outside allowed directories',
        userMessage: 'MCode could not list this folder. Try again.'
      }
    },
    async () => () => undefined
  )

  const error = await client.readDir({ dirPath: 'C:/Windows' }).catch((caught: unknown) => caught)

  assert.ok(error instanceof DesktopCommandError)
  assert.equal(error.code, 'fs_path_denied')
  assert.equal(error.context.operation, 'fs_read_dir')
  assert.equal(error.userMessage, 'MCode could not list this folder. Try again.')
})

test('watcher subscription filters non-fs payloads and unsubscription stops delivery', async () => {
  const listeners: ((event: { payload: unknown }) => void)[] = []
  let unlistenCalls = 0
  const client = createTauriFsClient(
    async () => null,
    async (_event, listener) => {
      listeners.push(listener)
      return () => {
        unlistenCalls += 1
      }
    }
  )

  const received: FsChangedPayload[] = []
  const subscription = await client.onFsChanged((payload) => received.push(payload))
  assert.equal(listeners.length, 1)

  listeners[0]?.({
    payload: { worktreePath: 'D:/repo', events: [{ kind: 'create', absolutePath: 'D:/repo/a' }] }
  })
  listeners[0]?.({ payload: { unrelated: true } })

  assert.deepEqual(received, [
    { worktreePath: 'D:/repo', events: [{ kind: 'create', absolutePath: 'D:/repo/a' }] }
  ])

  subscription()
  assert.equal(unlistenCalls, 1)
})

test('runtime wrapper routes to Electron without loading the Tauri client', async () => {
  let tauriLoads = 0
  let electronStats = 0
  const client = createRuntimeFsClient({
    isTauri: () => false,
    electron: {
      stat: () => {
        electronStats += 1
        return Promise.resolve({ size: 1, isDirectory: false, mtime: 1 })
      }
    } as unknown as FsApi,
    loadTauri: async () => {
      tauriLoads += 1
      throw new Error('Tauri client must not load under Electron')
    }
  })

  await client.stat({ filePath: 'D:/repo/a.md' })

  assert.equal(electronStats, 1)
  assert.equal(tauriLoads, 0)
})

test('runtime wrapper loads the Tauri client once and reuses it', async () => {
  let tauriLoads = 0
  let tauriExists = 0
  const client = createRuntimeFsClient({
    isTauri: () => true,
    electron: {
      pathExists: () => {
        throw new Error('Electron client must not run in Tauri')
      }
    } as unknown as FsApi,
    loadTauri: async () => {
      tauriLoads += 1
      return {
        pathExists: () => {
          tauriExists += 1
          return Promise.resolve(true)
        }
      } as unknown as FsApi
    }
  })

  assert.equal(await client.pathExists({ filePath: 'D:/repo' }), true)
  assert.equal(await client.pathExists({ filePath: 'D:/repo' }), true)
  assert.equal(tauriLoads, 1)
  assert.equal(tauriExists, 2)
})

test('the watcher event name matches the native emit contract', () => {
  assert.equal(FS_CHANGED_EVENT, 'fs://changed')
})

describe('smoke', () => {
  test('listener registration goes through the typed event source once per subscription', async () => {
    const listen = vi.fn(async () => () => undefined)
    const client = createTauriFsClient(async () => null, listen)

    await client.onFsChanged(failListener())

    expect(listen).toHaveBeenCalledTimes(1)
    expect(listen).toHaveBeenCalledWith(FS_CHANGED_EVENT, expect.any(Function))
  })
})
