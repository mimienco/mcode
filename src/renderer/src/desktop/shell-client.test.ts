import assert from 'node:assert/strict'
import { describe, expect, it as test } from 'vitest'
import { createRuntimeShellClient, createTauriShellClient, type ShellApi } from './shell-client'
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

test('typed client maps every method onto its shell command with camelCase args', async () => {
  const { calls, invoke } = recordedInvoke({
    shell_open_path: () => null,
    shell_open_in_file_manager: () => ({ ok: true }),
    shell_open_in_external_editor: () => ({ ok: true }),
    shell_open_url: () => null,
    shell_open_file_path: () => true,
    shell_open_file_uri: () => null,
    shell_path_exists: () => true,
    shell_pick_attachment: () => 'D:/pick/a.pdf',
    shell_pick_image: () => 'D:/pick/a.png',
    shell_pick_repo_icon_image: () => ({
      dataUrl: 'data:image/png;base64,AA==',
      fileName: 'a.png'
    }),
    shell_pick_audio: () => 'D:/pick/a.mp3',
    shell_pick_directory: () => 'D:/pick/dir',
    shell_copy_file: () => null
  })
  const client = createTauriShellClient(invoke)

  await client.openPath('D:/repo')
  assert.deepEqual(await client.openInFileManager('D:/repo'), { ok: true })
  assert.deepEqual(await client.openInExternalEditor({ path: 'D:/repo/a.md' }), { ok: true })
  await client.openUrl('https://example.com')
  assert.equal(await client.openFilePath('D:/repo/a.md'), true)
  await client.openFileUri('file:///D:/repo/a.md')
  assert.equal(await client.pathExists('D:/repo'), true)
  assert.equal(await client.pickAttachment(), 'D:/pick/a.pdf')
  assert.equal(await client.pickImage(), 'D:/pick/a.png')
  assert.deepEqual(await client.pickRepoIconImage(), {
    dataUrl: 'data:image/png;base64,AA==',
    fileName: 'a.png'
  })
  assert.equal(await client.pickAudio(), 'D:/pick/a.mp3')
  assert.equal(await client.pickDirectory({ defaultPath: 'D:/repo' }), 'D:/pick/dir')
  await client.copyFile({ srcPath: 'D:/repo/a.png', destPath: 'D:/repo/b.png' })

  assert.deepEqual(
    calls.map(({ command }) => command),
    [
      'shell_open_path',
      'shell_open_in_file_manager',
      'shell_open_in_external_editor',
      'shell_open_url',
      'shell_open_file_path',
      'shell_open_file_uri',
      'shell_path_exists',
      'shell_pick_attachment',
      'shell_pick_image',
      'shell_pick_repo_icon_image',
      'shell_pick_audio',
      'shell_pick_directory',
      'shell_copy_file'
    ]
  )
  assert.deepEqual(calls[0]?.args, { path: 'D:/repo' })
  assert.deepEqual(calls[2]?.args, { request: { path: 'D:/repo/a.md' } })
  assert.deepEqual(calls[3]?.args, { url: 'https://example.com' })
  assert.deepEqual(calls[6]?.args, { path: 'D:/repo' })
  assert.deepEqual(calls[11]?.args, { defaultPath: 'D:/repo' })
  assert.deepEqual(calls[12]?.args, { srcPath: 'D:/repo/a.png', destPath: 'D:/repo/b.png' })
})

test('pickers invoke without args and void methods still await the command', async () => {
  const { calls, invoke } = recordedInvoke({
    shell_open_url: () => null,
    shell_open_file_uri: () => null,
    shell_pick_attachment: () => null,
    shell_pick_image: () => null,
    shell_pick_repo_icon_image: () => null,
    shell_pick_audio: () => null
  })
  const client = createTauriShellClient(invoke)

  await client.openUrl('https://example.com/x')
  await client.openFileUri('file:///C:/tmp.txt')
  assert.equal(await client.pickAttachment(), null)
  // Void methods still forward their argument; zero-arg pickers send none.
  assert.deepEqual(calls[0]?.args, { url: 'https://example.com/x' })
  assert.deepEqual(calls[1]?.args, { uri: 'file:///C:/tmp.txt' })
  for (const call of calls.slice(2)) {
    assert.ok(call.args === undefined)
  }
})

test('copyFile forwards both path args with camelCase keys', async () => {
  const { calls, invoke } = recordedInvoke({ shell_copy_file: () => null })
  const client = createTauriShellClient(invoke)

  await client.copyFile({ srcPath: 'C:/x/src.png', destPath: 'C:/y/dest.png' })

  assert.deepEqual(calls[0]?.args, { srcPath: 'C:/x/src.png', destPath: 'C:/y/dest.png' })
})

test('pickDirectory omits defaultPath key when absent', async () => {
  const { calls, invoke } = recordedInvoke({ shell_pick_directory: () => 'C:/' })
  const client = createTauriShellClient(invoke)

  await client.pickDirectory({})

  assert.deepEqual(calls[0]?.args, { defaultPath: undefined })
})

test('openInExternalEditor passes the request object with connectionId intact', async () => {
  const { calls, invoke } = recordedInvoke({
    shell_open_in_external_editor: () => ({ ok: false, reason: 'remote-editor-unsupported' })
  })
  const client = createTauriShellClient(invoke)

  const result = await client.openInExternalEditor({
    path: 'D:/w/a.md',
    command: 'code',
    connectionId: 'conn-1'
  })

  assert.deepEqual(result, { ok: false, reason: 'remote-editor-unsupported' })
  assert.deepEqual(calls[0]?.args, {
    request: { path: 'D:/w/a.md', command: 'code', connectionId: 'conn-1' }
  })
})

test('invoke failures normalize onto DesktopCommandError with the native code', async () => {
  const client = createTauriShellClient(async () => {
    throw {
      code: 'shell_already_exists',
      context: { operation: 'shell_copy_file' },
      message: 'destination exists',
      userMessage: 'MCode could not copy the file. Try again.'
    }
  })

  await assert.rejects(client.copyFile({ srcPath: 'C:/a', destPath: 'C:/b' }), (error: unknown) => {
    assert.ok(error instanceof DesktopCommandError)
    assert.equal(error.code, 'shell_already_exists')
    assert.equal(error.context.operation, 'shell_copy_file')
    return true
  })
})

test('non-payload failures still normalize onto desktop_command_failed', async () => {
  const client = createTauriShellClient(async () => {
    throw new Error('invoke channel closed')
  })

  await assert.rejects(client.pathExists('C:/x'), (error: unknown) => {
    assert.ok(error instanceof DesktopCommandError)
    assert.equal(error.code, 'desktop_command_failed')
    assert.equal(error.context.operation, 'shell_path_exists')
    return true
  })
})

test('runtime wrapper routes to Electron without loading the Tauri client', async () => {
  let tauriLoads = 0
  let electronPicks = 0
  const client = createRuntimeShellClient({
    isTauri: () => false,
    electron: {
      pickAttachment: () => {
        electronPicks += 1
        return Promise.resolve('C:/electron-attachment.txt')
      }
    } as unknown as ShellApi,
    loadTauri: async () => {
      tauriLoads += 1
      throw new Error('Tauri client must not load under Electron')
    }
  })

  assert.equal(await client.pickAttachment(), 'C:/electron-attachment.txt')
  assert.equal(electronPicks, 1)
  assert.equal(tauriLoads, 0)
})

test('runtime wrapper loads the Tauri client once and reuses it', async () => {
  let tauriLoads = 0
  let tauriPicks = 0
  const client = createRuntimeShellClient({
    isTauri: () => true,
    electron: {
      pickDirectory: () => {
        throw new Error('Electron client must not run in Tauri')
      }
    } as unknown as ShellApi,
    loadTauri: async () => {
      tauriLoads += 1
      return {
        pickDirectory: () => {
          tauriPicks += 1
          return Promise.resolve('D:/tauri-dir')
        }
      } as unknown as ShellApi
    }
  })

  assert.equal(await client.pickDirectory({ defaultPath: 'D:/' }), 'D:/tauri-dir')
  assert.equal(await client.pickDirectory({}), 'D:/tauri-dir')
  assert.equal(tauriLoads, 1)
  assert.equal(tauriPicks, 2)
})

test('runtime wrapper forwards editor requests through the resolved client', async () => {
  const seen: unknown[] = []
  const client = createRuntimeShellClient({
    isTauri: () => true,
    electron: {} as unknown as ShellApi,
    loadTauri: async () =>
      ({
        openInExternalEditor: (request: unknown) => {
          seen.push(request)
          return Promise.resolve({ ok: true })
        }
      }) as unknown as ShellApi
  })

  assert.deepEqual(await client.openInExternalEditor({ path: 'D:/a.md' }), { ok: true })
  assert.deepEqual(seen, [{ path: 'D:/a.md' }])
})

describe('smoke', () => {
  test('tauri client exposes exactly the ShellApi method set', async () => {
    const client = createTauriShellClient(async () => null)
    assert.deepEqual(Object.keys(client).sort(), [
      'copyFile',
      'openFilePath',
      'openFileUri',
      'openInExternalEditor',
      'openInFileManager',
      'openPath',
      'openUrl',
      'pathExists',
      'pickAttachment',
      'pickAudio',
      'pickDirectory',
      'pickImage',
      'pickRepoIconImage'
    ])
    expect(Object.keys(client)).toHaveLength(13)
  })
})
