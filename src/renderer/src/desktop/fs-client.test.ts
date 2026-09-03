import assert from 'node:assert/strict'
import { describe, expect, it as test, vi } from 'vitest'
import {
  FS_CHANGED_EVENT,
  FS_LOG_TAIL_CHANGED_EVENT,
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
// ─── Task-C remainder: routing tests (one happy path + error passthrough) ───

const ERROR_PAYLOAD = {
  code: 'fs_transfer_unknown',
  context: { operation: 'fs_append_download_chunk', requestId: null, intent: null },
  message: 'Download session not found',
  userMessage: 'MCode could not complete this download. Try again.'
}

function failingInvokeClient(code: string) {
  return createTauriFsClient(
    async () => {
      throw { ...ERROR_PAYLOAD, code }
    },
    async () => () => undefined
  )
}

test('task-c methods route onto their fs_* commands with camelCase args', async () => {
  const { calls, invoke } = recordedInvoke({
    fs_authorize_external_path: () => null,
    fs_list_files: () => ['src/a.ts', 'src/b.ts'],
    fs_list_markdown_documents: () => [
      {
        filePath: 'D:/repo/README.md',
        relativePath: 'README.md',
        basename: 'README.md',
        name: 'README'
      }
    ],
    fs_import_external_paths: () => ({
      results: [
        {
          sourcePath: 'C:/drop/x.txt',
          status: 'imported',
          destPath: 'D:/repo/x.txt',
          kind: 'file',
          renamed: false
        }
      ]
    }),
    fs_stage_external_paths_for_runtime_upload: () => ({
      sources: [
        {
          sourcePath: 'C:/drop/x.txt',
          status: 'staged',
          name: 'x.txt',
          kind: 'file',
          entries: [{ relativePath: '', kind: 'file', contentBase64: 'aGk=' }]
        }
      ]
    }),
    fs_resolve_dropped_paths_for_agent: () => ({
      resolvedPaths: ['C:/drop/x.txt'],
      skipped: [],
      failed: []
    }),
    fs_download_remote_file: () => ({ canceled: true }),
    fs_download_remote_folder: () => ({ canceled: false, destinationPath: 'D:/out/folder' }),
    fs_save_downloaded_file: () => ({ canceled: false, destinationPath: 'D:/out/saved.bin' }),
    fs_start_download: () => ({
      canceled: false,
      transferId: 't-1',
      destinationPath: 'D:/out/chunk.bin'
    }),
    fs_append_download_chunk: () => ({ ok: true }),
    fs_finish_download: () => ({ canceled: false, destinationPath: 'D:/out/chunk.bin' }),
    fs_cancel_download: () => ({ ok: true }),
    fs_read_local_log_tail: () => ({
      contentBase64: 'aGk=',
      nextByteOffset: 2,
      fileSize: 2,
      fileIdentity: '0:0:1',
      hasMore: false,
      reset: false
    }),
    fs_start_local_log_tail: () => null,
    fs_stop_local_log_tail: () => null
  })
  const client = createTauriFsClient(invoke, async () => () => undefined)

  await client.authorizeExternalPath({ targetPath: 'C:/drop' })
  assert.deepEqual(
    await client.listFiles({
      rootPath: 'D:/repo',
      searchQuery: 'a',
      maxResults: 5,
      requestToken: 'tok'
    }),
    ['src/a.ts', 'src/b.ts']
  )
  assert.deepEqual(await client.listMarkdownDocuments({ rootPath: 'D:/repo' }), [
    {
      filePath: 'D:/repo/README.md',
      relativePath: 'README.md',
      basename: 'README.md',
      name: 'README'
    }
  ])
  const imported = await client.importExternalPaths({
    sourcePaths: ['C:/drop/x.txt'],
    destDir: 'D:/repo'
  })
  assert.equal(imported.results[0]?.status, 'imported')
  const staged = await client.stageExternalPathsForRuntimeUpload({ sourcePaths: ['C:/drop/x.txt'] })
  assert.equal(staged.sources[0]?.status, 'staged')
  assert.deepEqual(
    await client.resolveDroppedPathsForAgent({ paths: ['C:/drop/x.txt'], worktreePath: 'D:/repo' }),
    {
      resolvedPaths: ['C:/drop/x.txt'],
      skipped: [],
      failed: []
    }
  )
  assert.deepEqual(
    await client.downloadFile({ filePath: '/remote/f.bin', connectionId: 'ssh-1' }),
    { canceled: true }
  )
  assert.deepEqual(await client.downloadFolder({ dirPath: '/remote/dir', connectionId: 'ssh-1' }), {
    canceled: false,
    destinationPath: 'D:/out/folder'
  })
  assert.deepEqual(
    await client.saveDownloadedFile({
      suggestedName: 'saved.bin',
      content: 'aGk=',
      encoding: 'base64'
    }),
    {
      canceled: false,
      destinationPath: 'D:/out/saved.bin'
    }
  )
  assert.deepEqual(await client.startDownloadedFile({ suggestedName: 'chunk.bin' }), {
    canceled: false,
    transferId: 't-1',
    destinationPath: 'D:/out/chunk.bin'
  })
  assert.deepEqual(
    await client.appendDownloadedFileChunk({ transferId: 't-1', contentBase64: 'aGk=' }),
    { ok: true }
  )
  assert.deepEqual(await client.finishDownloadedFile({ transferId: 't-1' }), {
    canceled: false,
    destinationPath: 'D:/out/chunk.bin'
  })
  assert.deepEqual(await client.cancelDownloadedFile({ transferId: 't-1' }), { ok: true })
  assert.deepEqual(
    await client.readLocalLogTail({ filePath: 'D:/logs/a.log', fromByteOffset: 0 }),
    {
      contentBase64: 'aGk=',
      nextByteOffset: 2,
      fileSize: 2,
      fileIdentity: '0:0:1',
      hasMore: false,
      reset: false
    }
  )
  await client.startLocalLogTail({ filePath: 'D:/logs/a.log', subscriptionId: 'sub-1' })
  await client.stopLocalLogTail({ subscriptionId: 'sub-1' })

  assert.deepEqual(
    calls.map(({ command }) => command),
    [
      'fs_authorize_external_path',
      'fs_list_files',
      'fs_list_markdown_documents',
      'fs_import_external_paths',
      'fs_stage_external_paths_for_runtime_upload',
      'fs_resolve_dropped_paths_for_agent',
      'fs_download_remote_file',
      'fs_download_remote_folder',
      'fs_save_downloaded_file',
      'fs_start_download',
      'fs_append_download_chunk',
      'fs_finish_download',
      'fs_cancel_download',
      'fs_read_local_log_tail',
      'fs_start_local_log_tail',
      'fs_stop_local_log_tail'
    ]
  )
  assert.deepEqual(calls[0]?.args, { targetPath: 'C:/drop' })
  assert.deepEqual(calls[1]?.args, {
    rootPath: 'D:/repo',
    excludePaths: undefined,
    requestToken: 'tok',
    maxResults: 5,
    searchQuery: 'a'
  })
  assert.deepEqual(calls[6]?.args, { filePath: '/remote/f.bin', connectionId: 'ssh-1' })
  assert.deepEqual(calls[10]?.args, { transferId: 't-1', contentBase64: 'aGk=' })
  assert.deepEqual(calls[14]?.args, { filePath: 'D:/logs/a.log', subscriptionId: 'sub-1' })
})

test('task-c methods pass native error codes through DesktopCommandError', async () => {
  // Why: per-code clients created inside thunks — a fixed array of promises
  // would fire every rejection before the loop attaches its catch,
  // surfacing unhandled rejections, and one shared stub could not emit
  // per-command codes.
  const cases: [() => Promise<unknown>, string][] = [
    [
      () =>
        failingInvokeClient('fs_transfer_unknown').appendDownloadedFileChunk({
          transferId: 'gone',
          contentBase64: 'aGk='
        }),
      'fs_transfer_unknown'
    ],
    [
      () => failingInvokeClient('fs_transfer_unknown').finishDownloadedFile({ transferId: 'gone' }),
      'fs_transfer_unknown'
    ],
    [
      () =>
        failingInvokeClient('fs_remote_unsupported').downloadFile({
          filePath: '/remote/f.bin',
          connectionId: 'ssh-1'
        }),
      'fs_remote_unsupported'
    ],
    [
      () =>
        failingInvokeClient('fs_remote_unsupported').downloadFolder({
          dirPath: '/remote/dir',
          connectionId: 'ssh-1'
        }),
      'fs_remote_unsupported'
    ],
    [
      () =>
        failingInvokeClient('fs_remote_unsupported').resolveDroppedPathsForAgent({
          paths: ['x'],
          worktreePath: 'D:/repo',
          connectionId: 'ssh-1'
        }),
      'fs_remote_unsupported'
    ],
    [
      () =>
        failingInvokeClient('fs_invalid_argument').startLocalLogTail({
          filePath: 'D:/logs/a.log',
          subscriptionId: 'sub'
        }),
      'fs_invalid_argument'
    ]
  ]
  for (const [run, code] of cases) {
    const error = await run().catch((caught: unknown) => caught)
    assert.ok(error instanceof DesktopCommandError, `expected DesktopCommandError for ${code}`)
    assert.equal(error.code, code)
  }
})

test('log-tail subscription filters malformed payloads and unsubscribes', async () => {
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

  const received: { subscriptionId: string; eventType: string }[] = []
  const subscription = await client.onLocalLogTailChanged((payload) => received.push(payload))
  assert.equal(listeners.length, 1)

  listeners[0]?.({ payload: { subscriptionId: 'sub-1', eventType: 'change' } })
  listeners[0]?.({ payload: { subscriptionId: 'sub-1', eventType: 'bogus' } })
  listeners[0]?.({ payload: { unrelated: true } })

  assert.deepEqual(received, [{ subscriptionId: 'sub-1', eventType: 'change' }])

  subscription()
  assert.equal(unlistenCalls, 1)
})

test('the log-tail event name matches the native emit contract', () => {
  assert.equal(FS_LOG_TAIL_CHANGED_EVENT, 'fs://log-tail-changed')
})

test('runtime wrapper routes task-c methods to the resolved client once', async () => {
  let tauriLoads = 0
  let routed = 0
  const client = createRuntimeFsClient({
    isTauri: () => true,
    electron: {} as unknown as FsApi,
    loadTauri: async () => {
      tauriLoads += 1
      return {
        listFiles: () => {
          routed += 1
          return Promise.resolve(['a.ts'])
        },
        startDownloadedFile: () => {
          routed += 1
          return Promise.resolve({ canceled: true })
        },
        onLocalLogTailChanged: () => {
          routed += 1
          return Promise.resolve(() => undefined)
        }
      } as unknown as FsApi
    }
  })

  assert.deepEqual(await client.listFiles({ rootPath: 'D:/repo' }), ['a.ts'])
  assert.deepEqual(await client.startDownloadedFile({ suggestedName: 'x.bin' }), { canceled: true })
  await client.onLocalLogTailChanged(() => undefined)
  assert.equal(tauriLoads, 1)
  assert.equal(routed, 3)
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
