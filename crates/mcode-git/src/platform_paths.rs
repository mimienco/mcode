//! Windows/W path rules used by the git layer: long-path prefixes,
//! drive-letter case normalization, and WSL path translation.
//!
//! Kept as pure functions so future PTY/SSH layers can reuse them without
//! touching the git process runner.

use std::path::{Path, PathBuf};

/// Windows long-path limit (`MAX_PATH`).
const MAX_PATH: usize = 260;
/// Prefix that opts Win32 APIs into extended-length path semantics.
const EXTENDED_PREFIX: &str = r"\\?\";

/// Normalize drive-letter case to uppercase and return a canonical textual
/// form. `C:\foo` and `c:\foo` refer to the same location; git output and our
/// own comparisons should agree on one spelling.
pub fn normalize_drive_letter(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    let offset = if text.starts_with(EXTENDED_PREFIX) {
        EXTENDED_PREFIX.len()
    } else {
        0
    };
    if text.get(offset..offset + 2).is_some_and(is_drive_stem) {
        let mut normalized = text.to_owned();
        normalized.replace_range(
            offset..offset + 1,
            &text[offset..offset + 1].to_ascii_uppercase(),
        );
        PathBuf::from(normalized)
    } else {
        path.to_path_buf()
    }
}

fn is_drive_stem(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    bytes.len() == 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

/// Prepend the `\\?\` extended-length prefix when a Windows path exceeds
/// `MAX_PATH`. Already-prefixed and UNC paths pass through unchanged (UNC
/// needs the `\\?\UNC\` form, which callers must opt into explicitly).
pub fn maybe_long_path(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if text.starts_with(EXTENDED_PREFIX) || !needs_long_path(path) {
        return path.to_path_buf();
    }
    if text.starts_with(r"\\") {
        // UNC: `\\server\share` becomes `\\?\UNC\server\share`.
        let rest = text.trim_start_matches(r"\\");
        return PathBuf::from(format!(r"\\?\UNC\{rest}"));
    }
    if is_absolute_drive_path(&text) {
        PathBuf::from(format!("{EXTENDED_PREFIX}{}", text.replace('/', "\\")))
    } else {
        path.to_path_buf()
    }
}

/// True when the path would exceed the Win32 `MAX_PATH` limit without the
/// extended-length prefix.
pub fn needs_long_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    (is_absolute_drive_path(&text) || text.starts_with(r"\\"))
        && !text.starts_with(EXTENDED_PREFIX)
        && text.encode_utf16().count() >= MAX_PATH
}

fn is_absolute_drive_path(text: &str) -> bool {
    text.get(..2).is_some_and(is_drive_stem) && matches!(text.as_bytes().get(2), Some(b'/' | b'\\'))
}

/// A translated WSL path: the Windows original plus its Linux-side form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WslPath {
    pub windows: PathBuf,
    pub linux: String,
}

/// Translate a Windows path to its WSL form: `C:\repo\sub` → `/mnt/c/repo/sub`.
/// Already-POSIX paths (starting with `/`) pass through unchanged.
pub fn windows_to_wsl(path: &Path) -> Option<WslPath> {
    let text = path.as_os_str().to_string_lossy();
    if text.starts_with('/') {
        return Some(WslPath {
            windows: path.to_path_buf(),
            linux: text.into_owned(),
        });
    }
    let text = text.strip_prefix(EXTENDED_PREFIX).unwrap_or(&text);
    if !is_absolute_drive_path(text) {
        return None;
    }
    let drive = text.as_bytes()[0].to_ascii_lowercase() as char;
    let rest = text[3..].replace('\\', "/");
    let linux = if rest.is_empty() {
        format!("/mnt/{drive}")
    } else {
        format!("/mnt/{drive}/{rest}")
    };
    Some(WslPath {
        windows: PathBuf::from(text.replace('/', "\\")),
        linux,
    })
}

/// Translate a WSL path back to Windows: `/mnt/c/repo` → `C:\repo`.
/// Returns `None` for paths outside `/mnt/<drive>/` (e.g. pure Linux homes),
/// which have no Windows spelling.
pub fn wsl_to_windows(path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix('/')?;
    // `/mnt/<drive>/...` maps back to Windows; other POSIX roots have no
    // Windows spelling.
    let (mount, rest) = rest.split_once('/')?;
    if !mount.eq_ignore_ascii_case("mnt") {
        return None;
    }
    let (drive, remainder) = if rest.is_empty() {
        ("", "")
    } else {
        rest.split_once('/').unwrap_or((rest, ""))
    };
    if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    // Build the Windows text directly: `C:\` + remainder with separators.
    let mut windows_text = format!("{}:\\", drive.to_ascii_uppercase());
    if !remainder.is_empty() {
        windows_text.push_str(&remainder.replace('/', "\\"));
    }
    Some(PathBuf::from(windows_text))
}

/// Translate any path to the WSL side if it is Windows-shaped, else return the
/// POSIX text as-is. Convenience for PTY/SSH layers.
pub fn to_wsl_text(path: &Path) -> String {
    windows_to_wsl(path)
        .map(|t| t.linux)
        .unwrap_or_else(|| path.as_os_str().to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_path_rules_leave_posix_and_relative_names_unchanged() {
        for text in [
            format!("/repo/{}", "x".repeat(300)),
            format!("relative/{}", "x".repeat(300)),
        ] {
            assert_eq!(maybe_long_path(Path::new(&text)), PathBuf::from(&text));
            assert!(!needs_long_path(Path::new(&text)));
        }
        let short_unicode = format!(r"C:\{}", "\u{e9}".repeat(150));
        assert!(!needs_long_path(Path::new(&short_unicode)));
        assert!(windows_to_wsl(Path::new("C:relative")).is_none());
    }

    #[test]
    fn normalizes_drive_letter_case() {
        let lower = Path::new(r"c:\repo\sub");
        let upper = Path::new(r"C:\repo\sub");
        assert_eq!(normalize_drive_letter(lower), normalize_drive_letter(upper));
        assert_eq!(normalize_drive_letter(lower).to_str(), Some(r"C:\repo\sub"));
    }

    #[test]
    fn drive_letter_only_prefix_normalizes() {
        let p = Path::new("d:/x");
        let normalized = normalize_drive_letter(p).to_str().unwrap().to_string();
        assert_eq!(normalized.as_bytes()[0], b'D');
    }

    #[test]
    fn non_drive_prefixes_pass_through() {
        let unc = Path::new(r"\\server\share\x");
        assert_eq!(normalize_drive_letter(unc), unc.to_path_buf());
        let rel = Path::new("relative/path");
        assert_eq!(normalize_drive_letter(rel), rel.to_path_buf());
    }

    #[test]
    fn long_path_prefix_added_only_when_needed() {
        let short = Path::new(r"C:\short\path");
        assert_eq!(maybe_long_path(short), short.to_path_buf());
        let deep = format!(r"C:\{}", "d".repeat(300));
        let prefixed = maybe_long_path(Path::new(&deep));
        assert!(prefixed.to_str().unwrap().starts_with(r"\\?\C:\"));
        assert!(needs_long_path(Path::new(&deep)));
        assert!(!needs_long_path(short));
    }

    #[test]
    fn already_prefixed_path_not_double_prefixed() {
        let p = Path::new(r"\\?\C:\very\long");
        assert_eq!(maybe_long_path(p), p.to_path_buf());
    }

    #[test]
    fn unc_paths_get_unc_long_form() {
        let deep = format!(r"\\server\share\{}", "d".repeat(300));
        let prefixed = maybe_long_path(Path::new(&deep));
        assert!(
            prefixed
                .to_str()
                .unwrap()
                .starts_with(r"\\?\UNC\server\share\")
        );
    }

    #[test]
    fn windows_to_wsl_basic() {
        let t = windows_to_wsl(Path::new(r"C:\repo\sub")).unwrap();
        assert_eq!(t.linux, "/mnt/c/repo/sub");
        assert_eq!(t.windows.to_str(), Some(r"C:\repo\sub"));
    }

    #[test]
    fn windows_to_wsl_drive_root() {
        let t = windows_to_wsl(Path::new(r"D:\")).unwrap();
        assert_eq!(t.linux, "/mnt/d");
    }

    #[test]
    fn windows_to_wsl_posix_passthrough() {
        let t = windows_to_wsl(Path::new("/mnt/c/repo")).unwrap();
        assert_eq!(t.linux, "/mnt/c/repo");
    }

    #[test]
    fn windows_to_wsl_rejects_relative() {
        assert!(windows_to_wsl(Path::new("relative/path")).is_none());
    }

    #[test]
    fn wsl_to_windows_basic() {
        let p = wsl_to_windows("/mnt/c/repo/sub").unwrap();
        assert_eq!(p.to_str(), Some(r"C:\repo\sub"));
    }

    #[test]
    fn wsl_to_windows_drive_root() {
        let p = wsl_to_windows("/mnt/d").unwrap();
        assert_eq!(p.to_str(), Some(r"D:\"));
    }

    #[test]
    fn wsl_to_windows_rejects_non_mount() {
        assert!(wsl_to_windows("/home/user/repo").is_none());
        assert!(wsl_to_windows("relative").is_none());
        assert!(wsl_to_windows("/mnt/toolongdrive").is_none());
    }

    #[test]
    fn roundtrip_translation() {
        let original = Path::new(r"C:\repo\with space\sub");
        let wsl = windows_to_wsl(original).unwrap();
        let back = wsl_to_windows(&wsl.linux).unwrap();
        assert_eq!(back, original.to_path_buf());
    }

    #[test]
    fn to_wsl_text_handles_both_shapes() {
        assert_eq!(to_wsl_text(Path::new(r"C:\repo")), "/mnt/c/repo");
        assert_eq!(to_wsl_text(Path::new("/mnt/c/repo")), "/mnt/c/repo");
    }
}
