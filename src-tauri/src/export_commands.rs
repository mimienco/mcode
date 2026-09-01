//! Native export commands backing the renderer `ui.export` bridge
//! (`window.api.export.htmlToPdf`, Electron channel `export:html-to-pdf`).
//!
//! The original Electron main (`src/main/ipc/export.ts` +
//! `src/main/lib/html-to-pdf.ts`) wrote the HTML to a temp file, loaded it into
//! a hidden sandboxed `BrowserWindow`, waited for `<img>` loads, then called
//! `webContents.printToPDF({ printBackground: true, pageSize: 'A4', margins:
//! 0.75in })` and offered a native save dialog. Tauri's wry runtime on desktop
//! only exposes a modal `print()` — there is no programmatic print-to-pdf — so
//! this command reports an explicit unsupported error instead of pretending to
//! succeed.

use serde::Serialize;

/// Mirrors the Electron result union
/// `{ success: true; filePath } | { success: false; cancelled?; error? }`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportHtmlToPdfResult {
    success: bool,
    file_path: Option<String>,
    cancelled: bool,
    error: Option<String>,
}

impl ExportHtmlToPdfResult {
    fn error(message: &str) -> Self {
        Self {
            success: false,
            file_path: None,
            cancelled: false,
            error: Some(message.to_string()),
        }
    }
}

/// PDF export bridge (`export:html-to-pdf`). Currently unsupported: wry's
/// desktop WebView2/WebKitGTK bindings expose no programmatic print-to-pdf.
#[tauri::command(rename_as = "ui_exportHtmlToPdf")]
pub(crate) fn ui_export_html_to_pdf(html: String) -> ExportHtmlToPdfResult {
    if html.trim().is_empty() {
        return ExportHtmlToPdfResult::error("No content to export");
    }
    ExportHtmlToPdfResult::error(
        "PDF export requires print-to-pdf support not available in the current WebView (wry desktop); PDF export slice deferred until a solution exists",
    )
}

#[cfg(test)]
mod tests {
    use super::ExportHtmlToPdfResult;
    use super::ui_export_html_to_pdf;

    #[test]
    fn empty_html_reports_no_content_error() {
        let result = ui_export_html_to_pdf("   \n\t".to_string());
        assert_eq!(result, ExportHtmlToPdfResult::error("No content to export"));
    }

    #[test]
    fn non_empty_html_reports_explicit_unsupported_error() {
        let result = ui_export_html_to_pdf("<html><body>doc</body></html>".to_string());
        assert!(!result.success);
        assert!(result.file_path.is_none());
        assert!(!result.cancelled);
        let error = result
            .error
            .expect("unsupported export must carry an error");
        assert!(error.contains("print-to-pdf support not available"));
    }
}
