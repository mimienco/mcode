//! Content search via ripgrep and search-and-replace (spec §4 `fs:search` /
//! `fs:searchReplace`, §6 cancellation).

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ops;
use crate::scope::{AuthorizedPath, FsError};

/// Default search result cap (spec §4 `fs:search`).
pub const DEFAULT_SEARCH_MAX_RESULTS: usize = 2000;
/// Search timeout before results are marked truncated (spec §4 `fs:search`).
pub const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Per-file size cap for search-and-replace (spec §4 `fs:searchReplace`).
pub const REPLACE_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// One search hit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    pub line_number: u32,
    pub line_text: String,
}

/// Search outcome (spec §4 `fs:search`; `rgUnavailable` mirrors the original
/// flag rather than throwing).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub rg_unavailable: bool,
}

/// Per-file replace outcome.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ReplaceOutcome {
    Replaced { replacements: u32 },
    Skipped { reason: String },
    Failed { error: String },
}

/// Replace summary (spec §4 `fs:searchReplace`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceSummary {
    pub outcomes: HashMap<String, ReplaceOutcome>,
    pub cancelled: bool,
}

/// Cancellation token: registry-keyed, superseding the previous holder of the
/// same `(window_id, token)` (spec §6).
#[derive(Debug)]
pub struct CancellationToken {
    generation: u64,
    cancelled: Arc<AtomicU64>,
}

impl CancellationToken {
    /// Whether this generation was cancelled or superseded.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst) != self.generation
    }
}

#[derive(Debug, Default)]
struct TokenState {
    generations: HashMap<(u64, String), Arc<AtomicU64>>,
}

/// Registry mapping `(window_id, request_token)` to cancellation state.
///
/// Re-registering a `(window_id, token)` pair aborts the previous request
/// before installing the new one (spec §6: token reuse supersedes).
#[derive(Debug, Default)]
pub struct CancellationRegistry {
    state: Mutex<TokenState>,
}

impl CancellationRegistry {
    /// Registers `token` for `window_id`, superseding any prior request that
    /// used the same pair. Returns the fresh [`CancellationToken`].
    pub fn register(&self, window_id: u64, token: &str) -> CancellationToken {
        let mut state = self.state.lock().expect("cancellation registry lock");
        let key = (window_id, token.to_owned());
        if let Some(previous) = state.generations.get(&key) {
            previous.fetch_add(1, Ordering::SeqCst);
        }
        let counter = Arc::new(AtomicU64::new(0));
        state.generations.insert(key, counter.clone());
        CancellationToken {
            generation: 0,
            cancelled: counter,
        }
    }

    /// Explicit cancel for an already-issued token (spec §6 `fs_cancel_*`).
    pub fn cancel(&self, window_id: u64, token: &str) {
        let state = self.state.lock().expect("cancellation registry lock");
        if let Some(counter) = state.generations.get(&(window_id, token.to_owned())) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Resolves `rg` on PATH, returning `None` when it is unavailable (spec §4:
/// `SearchResult.rgUnavailable: true` instead of an error).
fn resolve_rg() -> Option<PathBuf> {
    if let Some(path_env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_env) {
            let candidate = dir.join(if cfg!(windows) { "rg.exe" } else { "rg" });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // WinGet installs CLIs into a per-user Links shim directory that may be
    // absent from PATH (matches the original's rg-availability probe
    // tolerance).
    if cfg!(windows)
        && let Some(home) = std::env::var_os("LOCALAPPDATA") {
            let shim = PathBuf::from(home)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("rg.exe");
            if shim.is_file() {
                return Some(shim);
            }
        }
    None
}

/// Escapes `query` as a literal for a ripgrep fixed-string search.
fn rg_command(root: &Path, rg_path: &Path, query: &str) -> Command {
    let mut command = Command::new(rg_path);
    command
        .arg("--json")
        .arg("--fixed-strings")
        .arg("--")
        .arg(query)
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

/// Parses one `rg --json` match line into a [`SearchMatch`].
fn parse_rg_match(line: &str) -> Option<SearchMatch> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "match" {
        return None;
    }
    let data = value.get("data")?;
    let path = data.get("path")?.get("text")?.as_str()?.to_owned();
    let line_number = data.get("line_number")?.as_u64()? as u32;
    let line_text = data
        .get("lines")?
        .get("text")?
        .as_str()?
        .trim_end_matches(['\n', '\r'])
        .to_owned();
    Some(SearchMatch {
        path,
        line_number,
        line_text,
    })
}

/// Searches `root` for `query`, streaming `rg --json` output with a 15 s
/// timeout (spec §4 `fs:search`).
///
/// When `rg` is not on PATH the result carries `rgUnavailable: true` and an
/// empty match list rather than an error. When the timeout elapses first the
/// result is marked `truncated: true`. Cancellation via `cancel` stops the
/// child and returns whatever matched so far.
pub fn search(
    root: &AuthorizedPath,
    query: &str,
    max_results: usize,
    cancel: &CancellationToken,
) -> SearchResult {
    let max_results = max_results.min(DEFAULT_SEARCH_MAX_RESULTS);
    let started = Instant::now();
    if cancel.is_cancelled() {
        return SearchResult {
            matches: Vec::new(),
            truncated: false,
            rg_unavailable: false,
        };
    }
    let Some(rg_path) = resolve_rg() else {
        return SearchResult {
            matches: Vec::new(),
            truncated: false,
            rg_unavailable: true,
        };
    };
    let mut command = rg_command(root.as_path(), &rg_path, query);
    command.current_dir(root.as_path());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return SearchResult {
                matches: Vec::new(),
                truncated: false,
                rg_unavailable: true,
            };
        }
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return SearchResult {
            matches: Vec::new(),
            truncated: false,
            rg_unavailable: true,
        };
    };

    let mut matches = Vec::new();
    let mut truncated = false;
    let mut reader = io::BufReader::new(stdout);
    let mut line = String::new();
    loop {
        if cancel.is_cancelled() || started.elapsed() >= SEARCH_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            truncated = started.elapsed() >= SEARCH_TIMEOUT;
            break;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Some(hit) = parse_rg_match(line.trim_end()) {
                    matches.push(hit);
                    if matches.len() >= max_results {
                        let _ = child.kill();
                        let _ = child.wait();
                        truncated = true;
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    if !matches.is_empty() || truncated || cancel.is_cancelled() {
        let _ = child.wait();
    }
    SearchResult {
        matches,
        truncated,
        rg_unavailable: false,
    }
}

/// Whether the byte buffer looks binary (contains a NUL in the first 8192
/// bytes, matching the ops-level probe).
fn looks_binary(bytes: &[u8]) -> bool {
    let probe_len = bytes.len().min(8192);
    bytes[..probe_len].contains(&0)
}

/// Replaces every literal occurrence of `query` with `replacement` in files
/// under `root` (spec §4 `fs:searchReplace`).
///
/// Per file: `skip_paths` are never written; files above
/// [`REPLACE_MAX_FILE_BYTES`] are skipped; binary files are skipped via the
/// probe; writes go through an atomic temp-file+rename preserving the file's
/// mode. Cancellation stops before the next file and the partial summary is
/// returned (spec §6).
pub fn search_replace(
    root: &AuthorizedPath,
    query: &str,
    replacement: &str,
    skip_paths: &[PathBuf],
    cancel: &CancellationToken,
) -> Result<ReplaceSummary, FsError> {
    if query.is_empty() {
        return Err(FsError::InvalidArgument("query".into()));
    }
    let root_path = root.as_path();
    let mut file_paths = Vec::new();
    collect_files(root_path, &mut file_paths)?;

    // Normalize skipPaths to the same canonical shape the enumerator
    // produces, so verbatim-prefix/casing differences cannot defeat the skip
    // (spec §4: dirty editor tabs are never written).
    let normalized_skips: Vec<PathBuf> = skip_paths
        .iter()
        .map(|skip| skip.canonicalize().unwrap_or_else(|_| skip.clone()))
        .collect();

    let mut outcomes = HashMap::new();
    for path in file_paths {
        if cancel.is_cancelled() {
            return Ok(ReplaceSummary {
                outcomes,
                cancelled: true,
            });
        }
        let display = path.display().to_string();
        let is_skipped = normalized_skips.iter().any(|skip| {
            if cfg!(windows) {
                path.to_string_lossy()
                    .eq_ignore_ascii_case(&skip.to_string_lossy())
            } else {
                path == *skip
            }
        });
        if is_skipped {
            outcomes.insert(
                display,
                ReplaceOutcome::Skipped {
                    reason: "skipped: dirty editor tab".into(),
                },
            );
            continue;
        }
        match replace_in_file(&path, query, replacement) {
            Ok(Some(count)) => outcomes.insert(
                display,
                ReplaceOutcome::Replaced {
                    replacements: count,
                },
            ),
            Ok(None) => continue,
            Err(error) => outcomes.insert(
                display,
                ReplaceOutcome::Failed {
                    error: error.to_string(),
                },
            ),
        };
    }
    Ok(ReplaceSummary {
        outcomes,
        cancelled: false,
    })
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), FsError> {
    const IGNORED: [&str; 9] = [
        ".git",
        "node_modules",
        "dist",
        "build",
        ".next",
        ".cache",
        "target",
        ".venv",
        "__pycache__",
    ];
    let entries = fs::read_dir(dir).map_err(FsError::from)?;
    for entry in entries {
        let entry = entry.map_err(FsError::from)?;
        let file_type = entry.file_type().map_err(FsError::from)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_dir() {
            if !IGNORED.contains(&name.as_str()) {
                collect_files(&entry.path(), out)?;
            }
        } else if file_type.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Replaces occurrences in one file, returning `Some(count)` when written and
/// `None` when untouched (no matches, binary, oversize, skip).
fn replace_in_file(path: &Path, query: &str, replacement: &str) -> Result<Option<u32>, FsError> {
    let bytes = match ops::read_capped(path, REPLACE_MAX_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(err @ FsError::TooLarge { .. }) => return Err(err),
        Err(err) => return Err(err),
    };
    if looks_binary(&bytes) {
        return Err(FsError::Io("binary file skipped".into()));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(FsError::Io("non-utf8 file skipped".into()));
    };
    let count = text.matches(query).count() as u32;
    if count == 0 {
        return Ok(None);
    }
    let updated = text.replace(query, replacement);
    let authorized = AuthorizedPath::assume_authorized(path.to_path_buf());
    ops::write_file_atomic(&authorized, &updated)?;
    Ok(Some(count))
}
