import { createDesktopInvoke } from './desktop-client'

// ─── Types (verbatim mirror of src/preload/api/shell-api.ts +
// src/shared/shell-open-types.ts) ────────────────────────────────────────────

export type ShellOpenExternalEditorRequest = {
  path: string
  command?: string
  connectionId?: string | null
}

export type ShellOpenPathFailureReason =
  | 'not-absolute'
  | 'not-found'
  | 'launch-failed'
  | 'remote-runtime-unsupported'
  | 'ssh-target-not-found'
  | 'ssh-target-invalid'
  | 'ssh-alias-required'
  | 'remote-editor-unsupported'

export type ShellOpenLocalPathFailureReason = Extract<
  ShellOpenPathFailureReason,
  'not-absolute' | 'not-found' | 'launch-failed' | 'remote-runtime-unsupported'
>

export type ShellOpenLocalPathResult =
  | { ok: true }
  | { ok: false; reason: ShellOpenLocalPathFailureReason }

export type ShellOpenExternalEditorResult =
  | { ok: true }
  | { ok: false; reason: Exclude<ShellOpenPathFailureReason, 'ssh-alias-required'> }
  | { ok: false; reason: 'ssh-alias-required'; host: string; port: number }

export type ShellApi = {
  openPath: (path: string) => Promise<void>
  openInFileManager: (path: string) => Promise<ShellOpenLocalPathResult>
  openInExternalEditor: (
    request: ShellOpenExternalEditorRequest
  ) => Promise<ShellOpenExternalEditorResult>
  openUrl: (url: string) => Promise<void>
  openFilePath: (path: string) => Promise<boolean>
  openFileUri: (uri: string) => Promise<void>
  pathExists: (path: string) => Promise<boolean>
  pickAttachment: () => Promise<string | null>
  pickImage: () => Promise<string | null>
  pickRepoIconImage: () => Promise<{
    dataUrl: string
    fileName: string
  } | null>
  pickAudio: () => Promise<string | null>
  pickDirectory: (args: { defaultPath?: string }) => Promise<string | null>
  copyFile: (args: { srcPath: string; destPath: string }) => Promise<void>
}

type ShellInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

/**
 * Builds the typed shell API over any invoke transport. Raw command strings
 * live only here; callers receive one typed method per `shell_*` command and
 * every failure is normalized onto `DesktopCommandError` via
 * `createDesktopInvoke`.
 */
export function createTauriShellClient(invoke: ShellInvoke): ShellApi {
  const call = createDesktopInvoke(invoke)
  return {
    openPath: async (path) => {
      await call('shell_open_path', { path })
    },
    openInFileManager: (path) =>
      call('shell_open_in_file_manager', { path }) as Promise<ShellOpenLocalPathResult>,
    openInExternalEditor: (request) =>
      call('shell_open_in_external_editor', { request }) as Promise<ShellOpenExternalEditorResult>,
    openUrl: async (url) => {
      await call('shell_open_url', { url })
    },
    openFilePath: (path) => call('shell_open_file_path', { path }) as Promise<boolean>,
    openFileUri: async (uri) => {
      await call('shell_open_file_uri', { uri })
    },
    pathExists: (path) => call('shell_path_exists', { path }) as Promise<boolean>,
    pickAttachment: () => call('shell_pick_attachment') as Promise<string | null>,
    pickImage: () => call('shell_pick_image') as Promise<string | null>,
    pickRepoIconImage: () =>
      call('shell_pick_repo_icon_image') as Promise<{ dataUrl: string; fileName: string } | null>,
    pickAudio: () => call('shell_pick_audio') as Promise<string | null>,
    pickDirectory: ({ defaultPath }) =>
      call('shell_pick_directory', { defaultPath }) as Promise<string | null>,
    copyFile: async ({ srcPath, destPath }) => {
      await call('shell_copy_file', { srcPath, destPath })
    }
  }
}

/**
 * Runtime-selection wrapper: resolves to the Tauri-native shell client lazily
 * on first use under Tauri; under Electron callers supply their own bridge
 * (fs-client.ts:191 pattern).
 */
export function createRuntimeShellClient({
  isTauri: isTauriRuntime,
  electron,
  loadTauri
}: {
  isTauri: () => boolean
  electron: ShellApi
  loadTauri: () => Promise<ShellApi>
}): ShellApi {
  let tauriClient: Promise<ShellApi> | undefined
  const resolveClient = (): Promise<ShellApi> => {
    if (!isTauriRuntime()) {
      return Promise.resolve(electron)
    }
    tauriClient ??= loadTauri()
    return tauriClient
  }
  return {
    openPath: async (path) => {
      await (await resolveClient()).openPath(path)
    },
    openInFileManager: async (path) => (await resolveClient()).openInFileManager(path),
    openInExternalEditor: async (request) => (await resolveClient()).openInExternalEditor(request),
    openUrl: async (url) => {
      await (await resolveClient()).openUrl(url)
    },
    openFilePath: async (path) => (await resolveClient()).openFilePath(path),
    openFileUri: async (uri) => {
      await (await resolveClient()).openFileUri(uri)
    },
    pathExists: async (path) => (await resolveClient()).pathExists(path),
    pickAttachment: async () => (await resolveClient()).pickAttachment(),
    pickImage: async () => (await resolveClient()).pickImage(),
    pickRepoIconImage: async () => (await resolveClient()).pickRepoIconImage(),
    pickAudio: async () => (await resolveClient()).pickAudio(),
    pickDirectory: async (args) => (await resolveClient()).pickDirectory(args),
    copyFile: async (args) => {
      await (await resolveClient()).copyFile(args)
    }
  }
}
