//! Git binary resolution, capability detection, and the process runner.
//!
//! Every invocation uses an argument vector — never a shell string — with a
//! bounded stdout reader and a per-call timeout.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;

/// Hard cap on captured stdout: 64 MB (a pathological `diff` or `log` output
/// beyond this is treated as an operation failure, not an OOM).
pub const MAX_STDOUT_BYTES: u64 = 64 * 1024 * 1024;

/// Timeout class for a git operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutClass {
    /// Cheap metadata probes: status, check-ignored, rev-list counts.
    Fast,
    /// Content-producing reads: diff, log, submodule status.
    Read,
    /// Network operations: fetch, pull, push.
    Network,
    /// Mutating operations: commit, worktree add/remove.
    Mutate,
}

impl TimeoutClass {
    pub fn duration(self) -> Duration {
        match self {
            Self::Fast => Duration::from_secs(30),
            Self::Read => Duration::from_secs(60),
            Self::Network => Duration::from_secs(300),
            Self::Mutate => Duration::from_secs(120),
        }
    }
}

/// Typed git-layer errors with stable snake_case codes for the renderer.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum GitError {
    /// No usable `git` executable found on PATH.
    GitBinaryMissing,
    /// The target path (or any parent) is not inside a git repository.
    GitNotARepository,
    /// An `index.lock` (or equivalent) exists and blocks the operation.
    GitLockExists,
    /// Merge/conflict markers or unresolved conflicts block the operation.
    GitDirtyConflict,
    /// Git exited non-zero for a reason not classified above.
    GitOperationFailed {
        command: String,
        stderr_redacted: String,
    },
    /// The operation exceeded its timeout budget.
    GitTimeout,
    /// The installed git lacks a capability the operation needs.
    GitUnsupportedCapability {
        capability: &'static str,
        version: String,
    },
}

impl GitError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::GitBinaryMissing => "git_binary_missing",
            Self::GitNotARepository => "git_not_a_repository",
            Self::GitLockExists => "git_lock_exists",
            Self::GitDirtyConflict => "git_dirty_conflict",
            Self::GitOperationFailed { .. } => "git_operation_failed",
            Self::GitTimeout => "git_timeout",
            Self::GitUnsupportedCapability { .. } => "git_unsupported_capability",
        }
    }

    /// Keep arguments such as remote credentials and commit messages out of diagnostics.
    pub fn operation_failed(argv: &[&OsStr], stderr: &[u8]) -> Self {
        let command = args_display(argv);
        Self::GitOperationFailed {
            command,
            stderr_redacted: redact_stderr(&String::from_utf8_lossy(stderr)),
        }
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitBinaryMissing => write!(f, "git binary not found on PATH"),
            Self::GitNotARepository => write!(f, "not a git repository"),
            Self::GitLockExists => write!(f, "git index is locked (index.lock exists)"),
            Self::GitDirtyConflict => write!(f, "operation blocked by unresolved conflict"),
            Self::GitOperationFailed {
                command,
                stderr_redacted,
            } => write!(f, "git {command} failed: {stderr_redacted}"),
            Self::GitTimeout => write!(f, "git operation timed out"),
            Self::GitUnsupportedCapability {
                capability,
                version,
            } => write!(
                f,
                "installed git {version} lacks required capability: {capability}"
            ),
        }
    }
}

impl std::error::Error for GitError {}

/// Redact user names and absolute paths from git stderr before surfacing it.
///
/// Git prints messages like `fatal: cannot change to 'C:\Users\me\repo': …`;
/// the shape of the error survives, the filesystem layout does not.
pub fn redact_stderr(stderr: &str) -> String {
    let mut out = String::with_capacity(stderr.len());
    for line in stderr.lines() {
        let mut redacted = String::new();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let rest = &line[i..];
            if let Some(len) = windows_path_len(rest) {
                redacted.push_str("<path>");
                i += len;
            } else if rest.starts_with('/') {
                // POSIX absolute path: consume up to the next delimiter.
                let end = rest.find(['\'', '"', ' ', ')', ',']).unwrap_or(rest.len());
                if end > 1 {
                    redacted.push_str("<path>");
                    i += end;
                } else {
                    redacted.push('/');
                    i += 1;
                }
            } else {
                // Copy one UTF-8 scalar (bytes are scanned by index).
                let ch_len = utf8_len(bytes[i]);
                redacted.push_str(&line[i..i + ch_len]);
                i += ch_len;
            }
        }
        out.push_str(&redacted);
        out.push('\n');
    }
    out
}

/// Match a Windows absolute path (`X:\...`) at the start of `rest`; return the
/// consumed byte length or `None`.
fn windows_path_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || bytes[2] != b'\\' || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let end = rest[3..]
        .find(['\'', '"', ' ', ')', ','])
        .map_or(rest.len(), |off| 3 + off);
    Some(end)
}

fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Detected git capabilities, cached after the first probe.
#[derive(Clone, Debug)]
pub struct GitCapabilities {
    pub version: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitCapabilities {
    /// `git worktree` landed in 2.5.0.
    pub fn supports_worktree(&self) -> bool {
        (self.major, self.minor, self.patch) >= (2, 5, 0)
    }

    /// `git status --porcelain=v2` landed in 2.11.0.
    pub fn supports_porcelain_v2(&self) -> bool {
        (self.major, self.minor, self.patch) >= (2, 11, 0)
    }

    /// `safe.directory` handling landed in 2.35.2 (CVE-2022-24765 fix).
    pub fn supports_safe_directory(&self) -> bool {
        (self.major, self.minor, self.patch) >= (2, 35, 2)
    }
}

/// Process runner: resolves the git binary once, executes argv vectors with
/// bounded capture and timeouts.
pub struct GitRunner {
    binary: PathBuf,
    capabilities: GitCapabilities,
    pub(crate) capability_cache: crate::capability_cache::GitCapabilityCache,
}

impl GitRunner {
    /// Resolve `git` from PATH and probe its version. Errors with
    /// `git_binary_missing` when nothing usable is found.
    pub fn discover() -> Result<Self, GitError> {
        let binary = resolve_git_binary().ok_or(GitError::GitBinaryMissing)?;
        let capabilities = probe_version(&binary)?;
        Ok(Self {
            binary,
            capabilities,
            capability_cache: Default::default(),
        })
    }

    /// Globally cached runner; safe to call from any command handler. The
    /// initializer resolves git at first use and caches the outcome for the
    /// process lifetime.
    pub fn shared() -> Result<&'static GitRunner, GitError> {
        static RUNNER: LazyLock<Result<GitRunner, GitError>> = LazyLock::new(GitRunner::discover);
        RUNNER.as_ref().map_err(Clone::clone)
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    pub fn capabilities(&self) -> &GitCapabilities {
        &self.capabilities
    }

    /// Require `git worktree` support or fail with `git_unsupported_capability`.
    pub fn require_worktree_support(&self) -> Result<(), GitError> {
        if self.capabilities.supports_worktree() {
            Ok(())
        } else {
            Err(GitError::GitUnsupportedCapability {
                capability: "git worktree (requires >= 2.5)",
                version: self.capabilities.version.clone(),
            })
        }
    }

    /// Run git with an argument vector in `cwd`, capture stdout/stderr with a
    /// byte bound, enforce `timeout`, and classify the failure.
    pub fn run(&self, cwd: &Path, timeout: Duration, args: &[&OsStr]) -> Result<Vec<u8>, GitError> {
        self.run_with_stdin(cwd, timeout, args, None)
    }

    /// Variant that writes `stdin` bytes to the child then closes the pipe.
    pub fn run_with_stdin(
        &self,
        cwd: &Path,
        timeout: Duration,
        args: &[&OsStr],
        stdin: Option<&[u8]>,
    ) -> Result<Vec<u8>, GitError> {
        let output = crate::process_capture::capture(
            &self.binary,
            cwd,
            args,
            stdin,
            timeout,
            MAX_STDOUT_BYTES,
        )?;
        classify_output(args, output, &[0])
    }

    pub(crate) fn run_accepting_exit_codes(
        &self,
        cwd: &Path,
        timeout: Duration,
        args: &[&OsStr],
        accepted: &[i32],
        stdin: Option<&[u8]>,
    ) -> Result<Vec<u8>, GitError> {
        let output = crate::process_capture::capture(
            &self.binary,
            cwd,
            args,
            stdin,
            timeout,
            MAX_STDOUT_BYTES,
        )?;
        classify_output(args, output, accepted)
    }
}

fn classify_output(
    args: &[&OsStr],
    output: crate::process_capture::Captured,
    accepted: &[i32],
) -> Result<Vec<u8>, GitError> {
    if output
        .status
        .code()
        .is_some_and(|code| accepted.contains(&code))
    {
        return Ok(output.stdout);
    }
    let stderr_text = String::from_utf8_lossy(&output.stderr).into_owned();
    if stderr_text.contains("index.lock") {
        return Err(GitError::GitLockExists);
    }
    if stderr_text.contains("not a git repository") {
        return Err(GitError::GitNotARepository);
    }
    if stderr_text.contains("unresolved conflict") || stderr_text.contains("CONFLICT") {
        return Err(GitError::GitDirtyConflict);
    }
    Err(GitError::GitOperationFailed {
        command: args_display(args),
        stderr_redacted: redact_stderr(&stderr_text),
    })
}

fn args_display(args: &[&OsStr]) -> String {
    let mut args = args.iter().map(|arg| arg.to_string_lossy());
    let command = args
        .find(|arg| !arg.starts_with('-'))
        .unwrap_or_else(|| "command".into());
    if command == "worktree"
        && let Some(subcommand) = args.next().filter(|subcommand| {
            matches!(
                subcommand.as_ref(),
                "add" | "list" | "remove" | "prune" | "repair" | "lock" | "unlock"
            )
        })
    {
        return format!("worktree {subcommand}");
    }
    command.into_owned()
}

fn resolve_git_binary() -> Option<PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn probe_version(binary: &Path) -> Result<GitCapabilities, GitError> {
    let cwd = std::env::current_dir().map_err(|_| GitError::GitBinaryMissing)?;
    let output = crate::process_capture::capture(
        binary,
        &cwd,
        &[OsStr::new("--version")],
        None,
        TimeoutClass::Fast.duration(),
        64 * 1024,
    )?;
    if !output.status.success() {
        return Err(GitError::GitBinaryMissing);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_version(&text).ok_or(GitError::GitBinaryMissing)
}

/// Parse `git version 2.44.0.windows.1` (or plain `2.44.0`) into components.
pub fn parse_version(text: &str) -> Option<GitCapabilities> {
    let line = text.lines().next()?;
    let rest = line.strip_prefix("git version")?.trim();
    let mut parts = rest.split_whitespace().next()?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some(GitCapabilities {
        version: rest.to_string(),
        major,
        minor,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apple_git_version_suffix() {
        let caps = parse_version("git version 2.39.5 (Apple Git-154)").unwrap();
        assert_eq!((caps.major, caps.minor, caps.patch), (2, 39, 5));
    }

    #[test]
    fn failed_command_diagnostics_do_not_echo_private_arguments() {
        let error = GitError::operation_failed(
            &[
                OsStr::new("push"),
                OsStr::new("https://user:password@example.test/repo"),
            ],
            b"failed",
        );
        assert!(!error.to_string().contains("password"));
        let error = GitError::operation_failed(
            &[
                OsStr::new("commit"),
                OsStr::new("-m"),
                OsStr::new("private message"),
            ],
            b"failed",
        );
        assert!(!error.to_string().contains("private message"));
    }

    #[test]
    fn parses_windows_suffixed_version() {
        let caps = parse_version("git version 2.44.0.windows.1\n").unwrap();
        assert_eq!((caps.major, caps.minor, caps.patch), (2, 44, 0));
        assert!(caps.supports_worktree());
        assert!(caps.supports_porcelain_v2());
        assert!(caps.supports_safe_directory());
    }

    #[test]
    fn parses_plain_version() {
        let caps = parse_version("git version 2.4.12").unwrap();
        assert!(!caps.supports_worktree());
        assert!(!caps.supports_porcelain_v2());
        assert!(!caps.supports_safe_directory());
        // Worktree landed 2.5, porcelain v2 2.11, safe.directory 2.35.2.
        let mid = parse_version("git version 2.10.1").unwrap();
        assert!(mid.supports_worktree());
        assert!(!mid.supports_porcelain_v2());
        let pre_cve = parse_version("git version 2.35.1").unwrap();
        assert!(pre_cve.supports_porcelain_v2());
        assert!(!pre_cve.supports_safe_directory());
    }

    #[test]
    fn rejects_garbage_version() {
        assert!(parse_version("not git output").is_none());
        assert!(parse_version("").is_none());
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(GitError::GitBinaryMissing.code(), "git_binary_missing");
        assert_eq!(GitError::GitNotARepository.code(), "git_not_a_repository");
        assert_eq!(GitError::GitLockExists.code(), "git_lock_exists");
        assert_eq!(GitError::GitDirtyConflict.code(), "git_dirty_conflict");
        assert_eq!(GitError::GitTimeout.code(), "git_timeout");
        assert_eq!(
            GitError::GitOperationFailed {
                command: "x".into(),
                stderr_redacted: String::new()
            }
            .code(),
            "git_operation_failed"
        );
    }

    #[test]
    fn stderr_redaction_hides_windows_paths() {
        let redacted = redact_stderr(
            "fatal: cannot change to 'C:\\Users\\alice\\secret': No such file or directory",
        );
        assert!(!redacted.contains("alice"));
        assert!(redacted.contains("<path>"));
    }

    #[test]
    fn stderr_redaction_hides_posix_paths() {
        let redacted = redact_stderr("fatal: /home/alice/secret: does not exist");
        assert!(!redacted.contains("alice"));
        assert!(redacted.contains("<path>"));
    }

    #[test]
    fn timeout_classes_are_bounded() {
        assert!(TimeoutClass::Fast.duration() < TimeoutClass::Read.duration());
        assert!(TimeoutClass::Read.duration() < TimeoutClass::Mutate.duration());
        assert!(TimeoutClass::Mutate.duration() < TimeoutClass::Network.duration());
    }

    #[test]
    fn shared_runner_resolves_on_hosts_with_git() {
        // The test host has git; this proves discovery works end to end.
        // Skips (loudly) only when git is genuinely absent.
        match resolve_git_binary() {
            Some(_) => assert!(GitRunner::shared().is_ok()),
            None => eprintln!("WARNING: no git binary on PATH; skipping shared-runner assertion"),
        }
    }
}
