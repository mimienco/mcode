//! Native clipboard commands backing the renderer `ui` clipboard bridge
//! (`src/renderer/src/desktop/tauri-clipboard-client.ts`).
//!
//! Electron served these over `clipboard:readText` / `clipboard:writeText` /
//! `clipboard:writeTerminalText`; the byte caps mirror
//! `CLIPBOARD_TEXT_READ_MAX_BYTES` / `CLIPBOARD_TEXT_WRITE_MAX_BYTES` from
//! `src/shared/clipboard-text.ts` so the renderer's too-large error handling
//! (`isClipboardTextTooLargeError`) keeps working unchanged.

use arboard::Clipboard;

/// Mirrors `CLIPBOARD_TEXT_READ_MAX_BYTES` (16 MiB).
const CLIPBOARD_TEXT_READ_MAX_BYTES: usize = 16 * 1024 * 1024;
/// Mirrors `CLIPBOARD_TEXT_WRITE_MAX_BYTES` (16 MiB).
const CLIPBOARD_TEXT_WRITE_MAX_BYTES: usize = 16 * 1024 * 1024;

fn clipboard_unavailable(operation: &str, error: arboard::Error) -> String {
    format!("{operation} clipboard unavailable: {error}")
}

/// Reads UTF-8 text from the system clipboard (`clipboard:readText`).
#[tauri::command(rename_as = "ui_clipboardReadText")]
pub(crate) async fn ui_clipboard_read_text(
    max_bytes: Option<usize>,
) -> Result<Option<String>, String> {
    let cap = max_bytes.unwrap_or(CLIPBOARD_TEXT_READ_MAX_BYTES);
    tauri::async_runtime::spawn_blocking(move || {
        let text = Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .map_err(|error| clipboard_unavailable("read", error))?;
        if text.len() > cap {
            return Err(format!("clipboard text exceeds {cap} bytes"));
        }
        Ok(Some(text))
    })
    .await
    .map_err(|error| format!("clipboard read task failed: {error}"))?
}

/// Writes UTF-8 text to the system clipboard (`clipboard:writeText`).
#[tauri::command(rename_as = "ui_clipboardWriteText")]
pub(crate) async fn ui_clipboard_write_text(text: String) -> Result<(), String> {
    if text.len() > CLIPBOARD_TEXT_WRITE_MAX_BYTES {
        return Err(format!(
            "clipboard text exceeds {CLIPBOARD_TEXT_WRITE_MAX_BYTES} bytes"
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(&text))
            .map_err(|error| clipboard_unavailable("write", error))
    })
    .await
    .map_err(|error| format!("clipboard write task failed: {error}"))?
}

/// Writes text and verifies the clipboard round-trip (`clipboard:writeTerminalText`).
#[tauri::command(rename_as = "ui_clipboardWriteTerminalText")]
pub(crate) async fn ui_clipboard_write_terminal_text(text: String) -> Result<(), String> {
    if text.len() > CLIPBOARD_TEXT_WRITE_MAX_BYTES {
        return Err(format!(
            "clipboard text exceeds {CLIPBOARD_TEXT_WRITE_MAX_BYTES} bytes"
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut clipboard =
            Clipboard::new().map_err(|error| clipboard_unavailable("write", error))?;
        clipboard
            .set_text(&text)
            .map_err(|error| clipboard_unavailable("write", error))?;
        let read_back = clipboard
            .get_text()
            .map_err(|error| clipboard_unavailable("verify", error))?;
        if read_back != text {
            return Err("clipboard verification failed: read-back text does not match".to_owned());
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("clipboard terminal-write task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_caps_match_shared_renderer_limits() {
        assert_eq!(CLIPBOARD_TEXT_READ_MAX_BYTES, 16 * 1024 * 1024);
        assert_eq!(CLIPBOARD_TEXT_WRITE_MAX_BYTES, 16 * 1024 * 1024);
    }

    #[cfg(feature = "clipboard-roundtrip-tests")]
    #[test]
    fn write_then_read_back_roundtrips() {
        let mut clipboard = Clipboard::new().expect("clipboard available in desktop test");
        let payload = "mcode-clipboard-roundtrip";
        clipboard.set_text(payload).expect("write succeeds");
        assert_eq!(clipboard.get_text().expect("read succeeds"), payload);
    }
}
