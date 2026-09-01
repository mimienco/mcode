import { invoke } from '@tauri-apps/api/core'

/**
 * Args mirroring `SaveClipboardImageAsTempFileArgs` from the Electron main
 * process (`orca/src/main/window/clipboard-image-temp-file.ts`).
 */
export type SaveClipboardImageAsTempFileArgs = {
  connectionId?: string | null
  runtimeEnvironmentId?: string | null
}

/**
 * Mirrors the Electron `clipboard:saveImageAsTempFile` handler:
 * reads the clipboard image, writes it to a temp file, and returns the path —
 * or null when the clipboard holds no image.
 *
 * The Tauri runtime has no built-in image clipboard yet. The `ui_clipboardReadImage`
 * Rust command does not exist yet either; until it lands, invoking it fails and we
 * surface that as "no image" (null) so the call shape stays stable for when the
 * Rust command arrives.
 */
export async function saveClipboardImageAsTempFile(
  args?: SaveClipboardImageAsTempFileArgs
): Promise<string | null> {
  const connectionId = args?.connectionId?.trim()
  if (connectionId && connectionId !== 'local') {
    // Why: SSH clipboard image paste requires the runtime upload slice
    // (see the Electron handler's SSH write path in
    // orca/src/main/window/clipboard-image-temp-file.ts) — not ported yet.
    console.info(
      'saveClipboardImageAsTempFile: remote clipboard image paste requires the runtime upload slice; skipping.'
    )
    return null
  }
  try {
    const path = await invoke<string>('ui_clipboardReadImage')
    return typeof path === 'string' && path.length > 0 ? path : null
  } catch (error) {
    console.info(
      'saveClipboardImageAsTempFile: ui_clipboardReadImage unavailable, treating clipboard as having no image.',
      error
    )
    return null
  }
}
