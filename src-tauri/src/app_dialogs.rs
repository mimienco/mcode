//! Native floating-workspace pickers ported from the Electron IPC handlers
//! `app:pickFloatingMarkdownDocument` and `app:pickFloatingWorkspaceDirectory`
//! (`orca/src/main/ipc/app.ts`). The markdown picker defaults to the
//! app-owned `floating-workspace` directory under the app data dir and
//! rejects non-markdown selections exactly like `isMarkdownDocumentName`.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

const FLOATING_WORKSPACE_DIRNAME: &str = "floating-workspace";
const NOT_MARKDOWN_MESSAGE: &str = "Selected file is not a markdown document.";

/// Mirrors `isMarkdownDocumentName`: the extension must be md, mdx, or
/// markdown (case-insensitive).
fn is_markdown_document_name(path: &std::path::Path) -> bool {
    // A trailing separator means the caller passed a directory; the Electron
    // picker only ever returns files, so reject defensively.
    if path.to_string_lossy().ends_with(['/', '\\']) {
        return false;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "mdx" | "markdown"
            )
        })
}

/// Mirrors `ensureDefaultFloatingWorkspacePath`: ensure the app-owned default
/// floating workspace directory exists and use it as the dialog start point.
fn ensure_default_floating_workspace_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .ok()?
        .join(FLOATING_WORKSPACE_DIRNAME);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn canonicalized(path: PathBuf) -> Result<String, String> {
    let canonical = std::fs::canonicalize(&path).map_err(|error| {
        format!(
            "Failed to resolve selected path {}: {error}",
            path.display()
        )
    })?;
    canonical.to_str().map(str::to_owned).ok_or_else(|| {
        format!(
            "Selected path is not valid Unicode: {}",
            canonical.display()
        )
    })
}

/// Port of `pickFloatingMarkdownDocument`: native open-file dialog filtered to
/// markdown documents, starting in the app-owned floating workspace.
/// Returns the canonicalized absolute path, or `None` when canceled.
///
/// Blocking dialog: callers must NOT run this on the main thread; the command
/// wrapper is `async` so it executes on the async runtime.
pub fn pick_floating_markdown_document<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<String>, String> {
    let mut builder = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md", "mdx", "markdown"]);
    if let Some(dir) = ensure_default_floating_workspace_dir(app) {
        builder = builder.set_directory(dir);
    }
    let Some(picked) = builder.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| format!("Selected file path is not usable: {error}"))?;
    if !is_markdown_document_name(&path) {
        return Err(NOT_MARKDOWN_MESSAGE.to_owned());
    }
    Ok(Some(canonicalized(path)?))
}

/// Port of `pickFloatingWorkspaceDirectory`: native open-directory dialog.
/// Returns the canonicalized absolute path, or `None` when canceled.
///
/// Blocking dialog: callers must NOT run this on the main thread; the command
/// wrapper is `async` so it executes on the async runtime.
pub fn pick_floating_workspace_directory<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<String>, String> {
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| format!("Selected directory path is not usable: {error}"))?;
    Ok(Some(canonicalized(path)?))
}

#[cfg(test)]
mod tests {
    use super::is_markdown_document_name;
    use std::path::Path;

    #[test]
    fn markdown_name_check_matches_electron() {
        for name in ["notes.md", "Notes.MDX", "a.markdown", "no-ext.md/inner.md"] {
            assert!(is_markdown_document_name(Path::new(name)), "{name}");
        }
        for name in [
            "notes.txt",
            "notes.markdowntwo",
            "notes",
            "notes.md/",
            "a.md.exe",
        ] {
            assert!(!is_markdown_document_name(Path::new(name)), "{name}");
        }
    }
}
