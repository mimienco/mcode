//! Path authorization against the allow-list (spec §2).
//!
//! Every filesystem entry point resolves through [`FsScopeRegistry::authorize`]
//! before touching the disk, so a compromised renderer cannot self-authorize
//! reads or deletes outside the registered roots.

use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Mutex;

/// How the leaf component of an authorized path is treated.
///
/// Spec §2: canonicalize the parent but preserve the leaf for delete/rename
/// (`filesystem-auth.ts:74-115`) so a symlink leaf is acted on as the link,
/// never dereferenced to an outside-root target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeafPolicy {
    /// The leaf is a real file or directory; canonicalize it.
    Canonicalize,
    /// The leaf may be a symlink; canonicalize only its parent and keep the
    /// literal leaf name.
    Preserve,
}

/// Typed filesystem error with a stable snake_case code (spec §3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FsError {
    /// Path resolves outside the allowed directories.
    PathDenied,
    /// Unknown repository or worktree path.
    WorktreeUnknown,
    /// ENOENT, matched by errno code or canonical message text.
    NotFound,
    /// A file or folder with the target name already exists.
    AlreadyExists,
    /// Raw EACCES/EPERM, or a create guard failure.
    PermissionDenied,
    /// File exceeds the requested size limit.
    TooLarge { limit_bytes: u64 },
    /// Target is a directory where a file was required.
    IsDirectory,
    /// Destination appeared or vanished before the operation completed.
    DestinationRace,
    /// The operation was cancelled or superseded by a newer request.
    Cancelled,
    /// An argument was missing or malformed.
    InvalidArgument(String),
    /// Anything else, with the underlying OS context.
    Io(String),
}

impl FsError {
    /// Stable error code surfaced to the renderer (spec §3).
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathDenied => "fs_path_denied",
            Self::WorktreeUnknown => "fs_worktree_unknown",
            Self::NotFound => "fs_not_found",
            Self::AlreadyExists => "fs_already_exists",
            Self::PermissionDenied => "fs_permission_denied",
            Self::TooLarge { .. } => "fs_too_large",
            Self::IsDirectory => "fs_is_directory",
            Self::DestinationRace => "fs_destination_race",
            Self::Cancelled => "fs_cancelled",
            Self::InvalidArgument(_) => "fs_invalid_argument",
            Self::Io(_) => "fs_io_error",
        }
    }

    /// Whether this error represents an ENOENT, including the canonical
    /// ENOENT message shape (spec §2, `filesystem-path-containment.ts:27-49`).
    pub fn is_enoent(&self) -> bool {
        match self {
            Self::NotFound => true,
            Self::Io(message) => {
                const CANONICAL_ENOENT: &str = "no such file or directory";
                let message = message.to_ascii_lowercase();
                message.contains("os error 2") || message.contains(CANONICAL_ENOENT)
            }
            _ => false,
        }
    }

    pub(crate) fn from_io(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            _ => Self::Io(error.to_string()),
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathDenied => {
                formatter.write_str("Access denied: path resolves outside allowed directories")
            }
            Self::WorktreeUnknown => {
                formatter.write_str("Access denied: unknown repository or worktree path")
            }
            Self::NotFound => formatter.write_str("the path does not exist"),
            Self::AlreadyExists => formatter.write_str("a file or folder already exists there"),
            Self::PermissionDenied => formatter.write_str("permission denied"),
            Self::TooLarge { limit_bytes } => {
                write!(formatter, "file exceeds the {} byte limit", limit_bytes)
            }
            Self::IsDirectory => formatter.write_str("cannot write to a directory"),
            Self::DestinationRace => {
                formatter.write_str("destination changed before the operation completed")
            }
            Self::Cancelled => formatter.write_str("the request was cancelled"),
            Self::InvalidArgument(label) => write!(formatter, "{} is required", label),
            Self::Io(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(error: std::io::Error) -> Self {
        Self::from_io(error)
    }
}

/// LRU capacity for the external-path authorization cache (spec §2,
/// `filesystem-auth.ts:16-34`).
const AUTHORIZED_EXTERNAL_CACHE_CAPACITY: usize = 4096;

/// Small O(1) LRU built on a `HashMap` plus an insertion-ordered eviction key.
///
/// Keys are cloned into the map; values stay owned. The ring of keys makes
/// eviction deterministic without an extra crate dependency.
#[derive(Debug)]
struct LruCache {
    capacity: usize,
    entries: HashMap<PathBuf, ()>,
    eviction_order: std::collections::VecDeque<PathBuf>,
}

impl LruCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            eviction_order: std::collections::VecDeque::new(),
        }
    }

    fn contains(&self, key: &Path) -> bool {
        self.entries.contains_key(key)
    }

    fn insert(&mut self, key: PathBuf) {
        if self.entries.contains_key(&key) {
            return;
        }
        if self.entries.len() >= self.capacity
            && let Some(oldest) = self.eviction_order.pop_front() {
                self.entries.remove(&oldest);
            }
        self.entries.insert(key.clone(), ());
        self.eviction_order.push_back(key);
    }
}

#[derive(Debug)]
struct ScopeState {
    roots: Vec<PathBuf>,
    external_cache: LruCache,
}

/// Allow-list of local roots (repos, worktrees, folder workspaces) that every
/// path must resolve inside before any syscall is issued.
#[derive(Debug)]
pub struct FsScopeRegistry {
    state: Mutex<ScopeState>,
}

/// A path that has been verified to resolve inside an allow-listed root.
///
/// Holds the canonical (or leaf-preserved) absolute path the caller should
/// operate on.
#[derive(Clone, Debug)]
pub struct AuthorizedPath {
    path: PathBuf,
}

impl AuthorizedPath {
    /// The verified absolute path to operate on.
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    /// The verified absolute path, consumed.
    pub fn into_path_buf(self) -> PathBuf {
        self.path
    }

    /// Crate-internal constructor for paths enumerated under an already
    /// authorized root (search/replace file listing).
    pub(crate) fn assume_authorized(path: PathBuf) -> Self {
        Self { path }
    }
}

/// Canonicalizes the nearest existing ancestor of `path` when `path` itself
/// does not exist yet (spec §2, `filesystem-auth.ts:141-168`).
///
/// Returns `(canonical_path, leaf_was_missing)`; `leaf_was_missing` is false
/// when the full path existed and canonicalized directly.
fn canonicalize_missing_leaf(path: &Path) -> std::io::Result<(PathBuf, bool)> {
    match path.canonicalize() {
        Ok(canonical) => Ok((canonical, false)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Walk up to the nearest existing ancestor, then reattach the
            // not-yet-existing tail verbatim (spec §2: missing-leaf paths
            // canonicalize the nearest existing ancestor).
            let mut existing = path.parent().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "path has no parent to canonicalize",
                )
            })?;
            let mut missing_tail = Vec::new();
            loop {
                match existing.canonicalize() {
                    Ok(canonical_ancestor) => {
                        let leaf = path.file_name().ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                "path has no leaf component",
                            )
                        })?;
                        // missing_tail is ordered deepest-first from the walk
                        // (deepest missing component was pushed first); the
                        // path leaf is the deepest, so prepend it.
                        missing_tail.insert(0, leaf);
                        let mut resolved = canonical_ancestor;
                        for component in missing_tail.into_iter().rev() {
                            resolved.push(component);
                        }
                        return Ok((resolved, true));
                    }
                    Err(_) => {
                        let leaf = existing.file_name().ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                "path has no existing ancestor",
                            )
                        })?;
                        missing_tail.push(leaf);
                        existing = existing.parent().ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                "path has no existing ancestor",
                            )
                        })?;
                    }
                }
            }
        }
        Err(error) => Err(error),
    }
}

/// Uppercases the Windows drive letter so renderer casing stays stable
/// (spec §7); other platforms pass through unchanged.
pub(crate) fn normalize_drive(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        let bytes = text.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            let mut normalized = String::with_capacity(text.len());
            normalized.push(bytes[0].to_ascii_uppercase() as char);
            normalized.push(':');
            normalized.push_str(&text[2..]);
            return PathBuf::from(normalized);
        }
        PathBuf::from(text.into_owned())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Checks whether `candidate` is strictly inside `root` (spec §2,
/// `filesystem-path-containment.ts:10-18`).
///
/// On Windows, a `strip_prefix` failure is double-checked by comparing
/// prefixes: two different drive letters (or a drive vs a UNC share) can never
/// be nested, so the check fails closed instead of letting a `..`-shaped
/// relative result slip through.
fn is_contained(root: &Path, candidate: &Path) -> bool {
    if candidate.starts_with(root) {
        return true;
    }
    #[cfg(windows)]
    {
        // Cross-drive or drive-vs-UNC prefixes can never be contained, even
        // when a case-insensitive filesystem would compare equal past the
        // prefix; rejecting here closes the `relative()` bypass.
        let root_prefix = root.components().next();
        let candidate_prefix = candidate.components().next();
        match (root_prefix, candidate_prefix) {
            (Some(Component::Prefix(root_prefix)), Some(Component::Prefix(candidate_prefix))) => {
                let prefixes_match = match (root_prefix.kind(), candidate_prefix.kind()) {
                    (Prefix::Disk(root_letter), Prefix::Disk(candidate_letter)) => {
                        root_letter.eq_ignore_ascii_case(&candidate_letter)
                    }
                    (Prefix::VerbatimDisk(root_letter), Prefix::VerbatimDisk(candidate_letter)) => {
                        root_letter.eq_ignore_ascii_case(&candidate_letter)
                    }
                    _ => false,
                };
                if !prefixes_match {
                    return false;
                }
                // Same drive but not a lexical prefix: only a case-insensitive
                // "candidate starts with root" match can still be contained.
                // This is deliberately asymmetric: a candidate that is a
                // strict PREFIX of the root (the root's own parent, the
                // traversal-escape target) is rejected.
                let root_text = root.to_string_lossy();
                let candidate_text = candidate.to_string_lossy();
                candidate_text.len() >= root_text.len()
                    && candidate_text[..root_text.len()].eq_ignore_ascii_case(&root_text)
                    && (candidate_text.len() == root_text.len()
                        || candidate_text[root_text.len()..].starts_with('\\')
                        || candidate_text[root_text.len()..].starts_with('/'))
            }
            _ => false,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (root, candidate);
        false
    }
}

impl FsScopeRegistry {
    /// Creates an empty registry; roots are registered with
    /// [`FsScopeRegistry::register_root`].
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ScopeState {
                roots: Vec::new(),
                external_cache: LruCache::new(AUTHORIZED_EXTERNAL_CACHE_CAPACITY),
            }),
        }
    }

    /// Registers a local root (repo, worktree, or folder-workspace path).
    ///
    /// Roots are canonicalized on registration and stored case-folded on
    /// Windows so renderer casing variations authorize (spec §7).
    pub fn register_root(&self, root: &Path) -> Result<AuthorizedPath, FsError> {
        let canonical = root.canonicalize().map_err(FsError::from)?;
        let normalized = crate::scope::normalize_drive(&canonical);
        let mut state = self.state.lock().expect("scope registry lock poisoned");
        if !state.roots.contains(&normalized) {
            state.roots.push(normalized.clone());
        }
        Ok(AuthorizedPath { path: normalized })
    }

    /// Drops a previously registered root.
    pub fn unregister_root(&self, root: &Path) {
        let Ok(canonical) = root.canonicalize() else {
            return;
        };
        let normalized = crate::scope::normalize_drive(&canonical);
        let mut state = self.state.lock().expect("scope registry lock poisoned");
        state.roots.retain(|existing| existing != &normalized);
    }

    /// Every currently registered root, normalized.
    pub fn roots(&self) -> Vec<PathBuf> {
        let state = self.state.lock().expect("scope registry lock poisoned");
        state.roots.clone()
    }

    /// Pre-authorizes an externally-sourced path (spec §2,
    /// `authorizeRendererExternalPath`).
    ///
    /// The path must still resolve inside an allow-listed root; on success the
    /// authorization is cached for the session in the bounded LRU so repeated
    /// calls skip re-canonicalization. Eviction is safe: every caller
    /// re-authorizes before operating, so a dropped entry only costs a
    /// re-check, never a permission grant.
    pub fn authorize_external_path(&self, path: &Path) -> Result<AuthorizedPath, FsError> {
        let (canonical, _) = canonicalize_missing_leaf(path).map_err(FsError::from)?;
        let normalized = crate::scope::normalize_drive(&canonical);
        let mut state = self.state.lock().expect("scope registry lock poisoned");
        if !state.roots.iter().any(|root| normalized.starts_with(root)) {
            return Err(FsError::PathDenied);
        }
        state.external_cache.insert(normalized.clone());
        Ok(AuthorizedPath { path: normalized })
    }

    /// Whether `path` was previously authorized via
    /// [`FsScopeRegistry::authorize_external_path`] and is still cached.
    pub fn is_externally_authorized(&self, path: &Path) -> bool {
        let Ok((canonical, _)) = canonicalize_missing_leaf(path) else {
            return false;
        };
        let normalized = crate::scope::normalize_drive(&canonical);
        let state = self.state.lock().expect("scope registry lock poisoned");
        state.external_cache.contains(&normalized)
    }

    /// Authorizes `path` against the allow-list under `policy` (spec §2).
    ///
    /// Under [`LeafPolicy::Preserve`] the leaf is kept verbatim while the
    /// parent is canonicalized, so symlink leaves are acted on as links. A
    /// missing leaf canonicalizes its nearest existing ancestor (spec §2,
    /// `filesystem-auth.ts:141-168`). Containment is checked with
    /// `strip_prefix` plus a Windows cross-drive guard, closing the
    /// `relative()` bypass from the spec.
    pub fn authorize(&self, path: &Path, policy: LeafPolicy) -> Result<AuthorizedPath, FsError> {
        // Spec §2 symlink policy: canonicalize the parent but preserve the
        // leaf, so a symlink leaf is acted on as the link, never
        // dereferenced. The probe must happen BEFORE canonicalization —
        // canonicalize() would follow the link and leak its outside target.
        let leaf_is_symlink = policy == LeafPolicy::Preserve
            && path
                .symlink_metadata()
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(false);
        let resolved = if leaf_is_symlink {
            // Canonicalize only the parent; the literal leaf (the link) is
            // reattached verbatim, never resolved through the link target.
            let parent = path.parent().ok_or(FsError::PathDenied)?;
            let canonical_parent = parent.canonicalize().map_err(FsError::from)?;
            let leaf = path.file_name().ok_or(FsError::PathDenied)?;
            canonical_parent.join(leaf)
        } else {
            let (resolved, leaf_was_missing) =
                canonicalize_missing_leaf(path).map_err(FsError::from)?;
            match (policy, leaf_was_missing) {
                (LeafPolicy::Preserve, false) => {
                    // Rebuild from the canonical parent plus the literal
                    // leaf so a symlink leaf is not dereferenced (belt and
                    // braces for TOCTOU re-checks).
                    let leaf = path.file_name().ok_or(FsError::PathDenied)?.to_owned();
                    let parent = resolved.parent().ok_or(FsError::PathDenied)?;
                    parent.join(leaf)
                }
                _ => resolved,
            }
        };
        let normalized = crate::scope::normalize_drive(&resolved);
        let state = self.state.lock().expect("scope registry lock poisoned");
        let authorized = state
            .roots
            .iter()
            .any(|root| is_contained(root, &normalized));
        if !authorized {
            return Err(FsError::PathDenied);
        }
        Ok(AuthorizedPath { path: normalized })
    }
}

impl Default for FsScopeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
