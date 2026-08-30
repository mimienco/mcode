use cap_std::ambient_authority;
#[cfg(unix)]
use cap_std::fs::OpenOptionsExt;
use cap_std::fs::{Dir, FileType, Metadata, MetadataExt, OpenOptions};
use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
#[cfg(windows)]
use std::path::Prefix;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock};
use std::time::UNIX_EPOCH;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScopeId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    pub name: String,
    pub kind: PathKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathMetadata {
    pub kind: PathKind,
    pub size: u64,
    pub modified_time_millis: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundedFileRead {
    Bytes(Vec<u8>),
    BinaryPrefix,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedFileSnapshot {
    pub read: BoundedFileRead,
    pub file_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsOperation {
    RegisterRoot,
    ResolvePath,
    ReadDirectory,
    ReadFile,
    WriteFileAtomic,
    Stat,
    PathExists,
}

#[derive(Debug)]
pub enum FsError {
    StateUnavailable,
    UnknownScope {
        scope_id: ScopeId,
    },
    InvalidTrustedRoot {
        path: PathBuf,
    },
    InvalidTargetPath {
        path: PathBuf,
    },
    RootNotDirectory {
        path: PathBuf,
    },
    PathOutsideScope {
        path: PathBuf,
    },
    SymlinkTraversalDenied {
        path: PathBuf,
    },
    FileTooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },
    TargetIsDirectory {
        path: PathBuf,
    },
    Io {
        operation: FsOperation,
        path: PathBuf,
        source: io::Error,
    },
}

impl FsError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::StateUnavailable => "fs_state_unavailable",
            Self::UnknownScope { .. } => "fs_scope_unknown",
            Self::InvalidTrustedRoot { .. } => "fs_invalid_trusted_root",
            Self::InvalidTargetPath { .. } => "fs_invalid_target_path",
            Self::RootNotDirectory { .. } => "fs_root_not_directory",
            Self::PathOutsideScope { .. } => "fs_path_outside_scope",
            Self::SymlinkTraversalDenied { .. } => "fs_symlink_traversal_denied",
            Self::FileTooLarge { .. } => "fs_file_too_large",
            Self::TargetIsDirectory { .. } => "fs_target_is_directory",
            Self::Io { .. } => "fs_io_failed",
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateUnavailable => write!(formatter, "filesystem scope state is unavailable"),
            Self::UnknownScope { scope_id } => write!(formatter, "unknown scope {scope_id:?}"),
            Self::InvalidTrustedRoot { path } => write!(
                formatter,
                "trusted root is not a normalized absolute path: {}",
                path.display()
            ),
            Self::InvalidTargetPath { path } => write!(
                formatter,
                "target is not a normalized absolute path: {}",
                path.display()
            ),
            Self::RootNotDirectory { path } => {
                write!(
                    formatter,
                    "trusted root is not a directory: {}",
                    path.display()
                )
            }
            Self::PathOutsideScope { path } => {
                write!(
                    formatter,
                    "path is outside registered roots: {}",
                    path.display()
                )
            }
            Self::SymlinkTraversalDenied { path } => {
                write!(formatter, "symlink traversal is denied: {}", path.display())
            }
            Self::FileTooLarge { path, size, limit } => write!(
                formatter,
                "file {} is {size} bytes and exceeds the {limit}-byte limit",
                path.display()
            ),
            Self::TargetIsDirectory { path } => {
                write!(formatter, "target is a directory: {}", path.display())
            }
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "{operation:?} failed for {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for FsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScopedFilesystem {
    inner: Arc<ScopedFilesystemInner>,
}

#[derive(Debug)]
struct ScopedFilesystemInner {
    next_scope_id: AtomicU64,
    roots: RwLock<Vec<RegisteredRoot>>,
}

#[derive(Clone, Debug)]
struct RegisteredRoot {
    scope_id: ScopeId,
    requested_path: PathBuf,
    canonical_path: PathBuf,
    directory: Arc<Dir>,
}

struct ScopedTarget {
    root: RegisteredRoot,
    relative_path: PathBuf,
    requested_path: PathBuf,
}

impl Default for ScopedFilesystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopedFilesystem {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScopedFilesystemInner {
                next_scope_id: AtomicU64::new(1),
                roots: RwLock::new(Vec::new()),
            }),
        }
    }

    pub fn register_root(&self, trusted_root: impl AsRef<Path>) -> Result<ScopeId, FsError> {
        let scope_id = self.next_scope_id();
        let root = prepare_root(trusted_root.as_ref(), scope_id)?;
        self.inner
            .roots
            .write()
            .map_err(|_| FsError::StateUnavailable)?
            .push(root);
        Ok(scope_id)
    }

    pub fn replace_roots<I, P>(&self, trusted_roots: I) -> Result<Vec<ScopeId>, FsError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut roots = Vec::new();
        let mut scope_ids = Vec::new();
        for trusted_root in trusted_roots {
            let scope_id = self.next_scope_id();
            if let Ok(root) = prepare_root(trusted_root.as_ref(), scope_id) {
                roots.push(root);
                scope_ids.push(scope_id);
            }
        }

        *self
            .inner
            .roots
            .write()
            .map_err(|_| FsError::StateUnavailable)? = roots;
        Ok(scope_ids)
    }

    pub fn unregister_scope(&self, scope_id: ScopeId) -> Result<(), FsError> {
        let mut roots = self
            .inner
            .roots
            .write()
            .map_err(|_| FsError::StateUnavailable)?;
        let root_index = roots
            .iter()
            .position(|root| root.scope_id == scope_id)
            .ok_or(FsError::UnknownScope { scope_id })?;
        roots.remove(root_index);
        Ok(())
    }

    pub fn read_dir(&self, target_path: &Path) -> Result<Vec<DirectoryEntry>, FsError> {
        let target = self.resolve(target_path)?;
        let directory = target
            .root
            .directory
            .read_dir(&target.relative_path)
            .map_err(|source| scoped_io_error(FsOperation::ReadDirectory, &target, source))?;
        let mut entries = Vec::new();
        for entry_result in directory {
            let entry = entry_result
                .map_err(|source| scoped_io_error(FsOperation::ReadDirectory, &target, source))?;
            let name = entry.file_name().into_string().map_err(|_| FsError::Io {
                operation: FsOperation::ReadDirectory,
                path: target.requested_path.clone(),
                source: io::Error::new(io::ErrorKind::InvalidData, "filename is not valid UTF-8"),
            })?;
            let file_type = entry
                .file_type()
                .map_err(|source| scoped_io_error(FsOperation::ReadDirectory, &target, source))?;
            entries.push(DirectoryEntry {
                name,
                kind: path_kind(file_type),
            });
        }
        entries.sort_by(compare_directory_entries);
        Ok(entries)
    }

    pub fn stat(&self, target_path: &Path) -> Result<PathMetadata, FsError> {
        let target = self.resolve(target_path)?;
        let metadata = target
            .root
            .directory
            .metadata(&target.relative_path)
            .map_err(|source| scoped_io_error(FsOperation::Stat, &target, source))?;
        Ok(PathMetadata {
            kind: path_kind(metadata.file_type()),
            size: metadata.len(),
            modified_time_millis: modified_time_millis(&metadata, &target.requested_path)?,
        })
    }

    pub fn path_exists(&self, target_path: &Path) -> Result<bool, FsError> {
        let target = self.resolve(target_path)?;
        target
            .root
            .directory
            .try_exists(&target.relative_path)
            .map_err(|source| scoped_io_error(FsOperation::PathExists, &target, source))
    }

    pub fn read_file_bounded(
        &self,
        target_path: &Path,
        max_bytes: usize,
        binary_probe_bytes: Option<usize>,
    ) -> Result<BoundedFileRead, FsError> {
        self.read_file_snapshot_bounded(target_path, max_bytes, binary_probe_bytes)
            .map(|snapshot| snapshot.read)
    }

    pub fn read_file_snapshot_bounded(
        &self,
        target_path: &Path,
        max_bytes: usize,
        binary_probe_bytes: Option<usize>,
    ) -> Result<BoundedFileSnapshot, FsError> {
        let target = self.resolve(target_path)?;
        let mut file = target
            .root
            .directory
            .open(&target.relative_path)
            .map_err(|source| scoped_io_error(FsOperation::ReadFile, &target, source))?;
        let metadata = file
            .metadata()
            .map_err(|source| scoped_io_error(FsOperation::ReadFile, &target, source))?;
        if metadata.is_dir() {
            return Err(FsError::TargetIsDirectory {
                path: target.requested_path,
            });
        }
        enforce_file_size(&target.requested_path, metadata.len(), max_bytes)?;
        let file_identity = file_identity(&metadata);

        let probe_limit = binary_probe_bytes.unwrap_or(0).min(max_bytes);
        let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes as u64) as usize);
        if probe_limit > 0 {
            (&mut file)
                .take(probe_limit as u64)
                .read_to_end(&mut bytes)
                .map_err(|source| scoped_io_error(FsOperation::ReadFile, &target, source))?;
            if bytes.contains(&0) {
                return Ok(BoundedFileSnapshot {
                    read: BoundedFileRead::BinaryPrefix,
                    file_identity,
                });
            }
        }

        let remaining_with_sentinel = max_bytes.saturating_sub(bytes.len()).saturating_add(1);
        (&mut file)
            .take(remaining_with_sentinel as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| scoped_io_error(FsOperation::ReadFile, &target, source))?;
        enforce_file_size(&target.requested_path, bytes.len() as u64, max_bytes)?;
        Ok(BoundedFileSnapshot {
            read: BoundedFileRead::Bytes(bytes),
            file_identity,
        })
    }

    pub fn write_file_atomic(
        &self,
        target_path: &Path,
        contents: &[u8],
        max_bytes: usize,
    ) -> Result<(), FsError> {
        enforce_file_size(target_path, contents.len() as u64, max_bytes)?;
        let target = self.resolve_write_target(target_path)?;
        let target_name = target
            .relative_path
            .file_name()
            .ok_or_else(|| FsError::TargetIsDirectory {
                path: target.requested_path.clone(),
            })?
            .to_os_string();
        let parent_path = target
            .relative_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty());
        let parent_path = parent_path.unwrap_or_else(|| Path::new("."));
        let parent = target
            .root
            .directory
            .open_dir(parent_path)
            .map_err(|source| scoped_io_error(FsOperation::WriteFileAtomic, &target, source))?;
        let permissions = match parent.symlink_metadata(&target_name) {
            Ok(metadata) if metadata.is_dir() => {
                return Err(FsError::TargetIsDirectory {
                    path: target.requested_path,
                });
            }
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(FsError::SymlinkTraversalDenied {
                    path: target.requested_path,
                });
            }
            Ok(metadata) => Some(metadata.permissions()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(scoped_io_error(
                    FsOperation::WriteFileAtomic,
                    &target,
                    source,
                ));
            }
        };

        let (temporary_name, mut temporary_file) = create_temporary_file(&parent, &target)?;
        let result = (|| {
            if let Some(permissions) = permissions {
                // Apply the target's access policy before payload bytes exist;
                // a named temporary file must never be broader than the file
                // it is going to replace.
                temporary_file
                    .set_permissions(permissions)
                    .map_err(|source| {
                        scoped_io_error(FsOperation::WriteFileAtomic, &target, source)
                    })?;
            }
            temporary_file
                .write_all(contents)
                .and_then(|()| temporary_file.sync_all())
                .map_err(|source| scoped_io_error(FsOperation::WriteFileAtomic, &target, source))?;
            drop(temporary_file);
            parent
                .rename(&temporary_name, &parent, &target_name)
                .and_then(|()| sync_directory(&parent))
                .map_err(|source| scoped_io_error(FsOperation::WriteFileAtomic, &target, source))
        })();
        if result.is_err() {
            let _ = parent.remove_file(&temporary_name);
        }
        result
    }

    fn resolve_write_target(&self, target_path: &Path) -> Result<ScopedTarget, FsError> {
        let mut target = self.resolve(target_path)?;
        let canonical =
            match target.root.directory.canonicalize(&target.relative_path) {
                Ok(path) => path,
                Err(source) if source.kind() == io::ErrorKind::NotFound => {
                    let name = target.relative_path.file_name().ok_or_else(|| {
                        FsError::TargetIsDirectory {
                            path: target.requested_path.clone(),
                        }
                    })?;
                    let parent = target
                        .relative_path
                        .parent()
                        .filter(|path| !path.as_os_str().is_empty())
                        .unwrap_or_else(|| Path::new("."));
                    let mut canonical_parent =
                        target
                            .root
                            .directory
                            .canonicalize(parent)
                            .map_err(|source| {
                                scoped_io_error(FsOperation::WriteFileAtomic, &target, source)
                            })?;
                    canonical_parent.push(name);
                    canonical_parent
                }
                Err(source) => {
                    return Err(scoped_io_error(
                        FsOperation::WriteFileAtomic,
                        &target,
                        source,
                    ));
                }
            };
        target.relative_path = canonical;
        Ok(target)
    }

    fn resolve(&self, target_path: &Path) -> Result<ScopedTarget, FsError> {
        let mut target = self.resolve_without_symlinks(target_path)?;
        target.relative_path =
            rewrite_absolute_symlinks(&target.root, &target.relative_path, &target.requested_path)?;
        Ok(target)
    }

    fn resolve_without_symlinks(&self, target_path: &Path) -> Result<ScopedTarget, FsError> {
        if !is_normalized_absolute(target_path) {
            return Err(FsError::InvalidTargetPath {
                path: target_path.to_path_buf(),
            });
        }

        let roots = self
            .inner
            .roots
            .read()
            .map_err(|_| FsError::StateUnavailable)?;
        let (root, relative_path) = roots
            .iter()
            .filter_map(|root| {
                relative_to(target_path, &root.requested_path)
                    .or_else(|| relative_to(target_path, &root.canonical_path))
                    .map(|relative_path| (root, relative_path))
            })
            .max_by_key(|(root, _)| root.canonical_path.components().count())
            .map(|(root, relative_path)| (root.clone(), relative_path))
            .ok_or_else(|| FsError::PathOutsideScope {
                path: target_path.to_path_buf(),
            })?;
        let relative_path = if relative_path.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            relative_path
        };
        Ok(ScopedTarget {
            root,
            relative_path,
            requested_path: target_path.to_path_buf(),
        })
    }

    fn next_scope_id(&self) -> ScopeId {
        ScopeId(
            self.inner
                .next_scope_id
                .fetch_add(1, AtomicOrdering::Relaxed),
        )
    }
}

#[cfg(unix)]
fn file_identity(metadata: &Metadata) -> String {
    format!("{}:{}:0", metadata.dev(), metadata.ino())
}

#[cfg(windows)]
fn file_identity(metadata: &Metadata) -> String {
    // The creation timestamp is stable across append-only growth and changes
    // when an atomic replacement installs a newly-created file.
    format!("0:0:{}", metadata.creation_time())
}

#[cfg(not(any(unix, windows)))]
fn file_identity(metadata: &Metadata) -> String {
    let created = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    format!("0:0:{created}")
}

fn enforce_file_size(path: &Path, size: u64, max_bytes: usize) -> Result<(), FsError> {
    let limit = max_bytes as u64;
    if size > limit {
        Err(FsError::FileTooLarge {
            path: path.to_path_buf(),
            size,
            limit,
        })
    } else {
        Ok(())
    }
}

fn create_temporary_file(
    parent: &Dir,
    target: &ScopedTarget,
) -> Result<(PathBuf, cap_std::fs::File), FsError> {
    for _ in 0..32 {
        let mut random_bytes = [0_u8; 16];
        getrandom::fill(&mut random_bytes).map_err(|source| FsError::Io {
            operation: FsOperation::WriteFileAtomic,
            path: target.requested_path.clone(),
            source: io::Error::other(format!("temporary filename randomness failed: {source}")),
        })?;
        let name = PathBuf::from(format!(
            ".mcode-write-{:032x}.tmp",
            u128::from_ne_bytes(random_bytes)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match parent.open_with(&name, &options) {
            Ok(file) => return Ok((name, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(scoped_io_error(
                    FsOperation::WriteFileAtomic,
                    target,
                    source,
                ));
            }
        }
    }
    Err(FsError::Io {
        operation: FsOperation::WriteFileAtomic,
        path: target.requested_path.clone(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate an atomic-write temporary file",
        ),
    })
}

#[cfg(unix)]
fn sync_directory(directory: &Dir) -> io::Result<()> {
    Dir::reopen_dir(directory)?.into_std_file().sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Dir) -> io::Result<()> {
    // Windows does not expose a portable directory-fsync primitive. The
    // capability-relative rename remains atomic after the temp file is flushed.
    Ok(())
}

fn rewrite_absolute_symlinks(
    root: &RegisteredRoot,
    relative_path: &Path,
    requested_path: &Path,
) -> Result<PathBuf, FsError> {
    let mut resolved = relative_path.to_path_buf();
    for _ in 0..40 {
        let components = resolved
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => Some(value.to_os_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut prefix = PathBuf::new();
        let mut replacement = None;

        for (index, component) in components.iter().enumerate() {
            prefix.push(component);
            let metadata = match root.directory.symlink_metadata(&prefix) {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(resolved),
                Err(source) if source.kind() == io::ErrorKind::PermissionDenied => {
                    return Err(FsError::PathOutsideScope {
                        path: requested_path.to_path_buf(),
                    });
                }
                Err(source) => {
                    return Err(FsError::Io {
                        operation: FsOperation::ResolvePath,
                        path: requested_path.to_path_buf(),
                        source,
                    });
                }
            };
            if !metadata.file_type().is_symlink() {
                continue;
            }

            let link_target = root
                .directory
                .read_link_contents(&prefix)
                .map_err(|source| FsError::Io {
                    operation: FsOperation::ResolvePath,
                    path: requested_path.to_path_buf(),
                    source,
                })?;
            if !link_target.is_absolute() {
                continue;
            }
            // Parent components in an absolute link are resolved after every
            // preceding symlink by the OS. Lexically collapsing them could turn
            // an outside traversal into an unrelated inside path, so fail closed.
            let Some(mut inside_target) = absolute_link_target_inside_root(root, &link_target)
            else {
                return Err(FsError::PathOutsideScope {
                    path: requested_path.to_path_buf(),
                });
            };
            inside_target.extend(components.iter().skip(index + 1));
            replacement = Some(inside_target);
            break;
        }

        match replacement {
            Some(next) => resolved = next,
            None => return Ok(resolved),
        }
    }
    Err(FsError::Io {
        operation: FsOperation::ResolvePath,
        path: requested_path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidData, "too many symbolic links"),
    })
}

fn absolute_link_target_inside_root(root: &RegisteredRoot, link_target: &Path) -> Option<PathBuf> {
    is_normalized_absolute(link_target)
        .then(|| {
            relative_to(link_target, &root.canonical_path)
                .or_else(|| relative_to(link_target, &root.requested_path))
        })
        .flatten()
}

fn prepare_root(path: &Path, scope_id: ScopeId) -> Result<RegisteredRoot, FsError> {
    let requested_path = path.to_path_buf();
    if !is_normalized_absolute(&requested_path) {
        return Err(FsError::InvalidTrustedRoot {
            path: requested_path,
        });
    }

    let directory =
        Dir::open_ambient_dir(&requested_path, ambient_authority()).map_err(|source| {
            if source.kind() == io::ErrorKind::NotADirectory {
                FsError::RootNotDirectory {
                    path: requested_path.clone(),
                }
            } else {
                FsError::Io {
                    operation: FsOperation::RegisterRoot,
                    path: requested_path.clone(),
                    source,
                }
            }
        })?;
    let canonical_path = fs::canonicalize(&requested_path).map_err(|source| FsError::Io {
        operation: FsOperation::RegisterRoot,
        path: requested_path.clone(),
        source,
    })?;
    let canonical_directory =
        Dir::open_ambient_dir(&canonical_path, ambient_authority()).map_err(|source| {
            FsError::Io {
                operation: FsOperation::RegisterRoot,
                path: requested_path.clone(),
                source,
            }
        })?;
    let captured_handle = directory
        .try_clone()
        .and_then(|directory| same_file::Handle::from_file(directory.into_std_file()))
        .map_err(|source| FsError::Io {
            operation: FsOperation::RegisterRoot,
            path: requested_path.clone(),
            source,
        })?;
    let canonical_handle = same_file::Handle::from_file(canonical_directory.into_std_file())
        .map_err(|source| FsError::Io {
            operation: FsOperation::RegisterRoot,
            path: requested_path.clone(),
            source,
        })?;
    if captured_handle != canonical_handle {
        return Err(FsError::InvalidTrustedRoot {
            path: requested_path,
        });
    }
    Ok(RegisteredRoot {
        scope_id,
        requested_path,
        canonical_path,
        directory: Arc::new(directory),
    })
}

fn scoped_io_error(operation: FsOperation, target: &ScopedTarget, source: io::Error) -> FsError {
    if source.kind() == io::ErrorKind::PermissionDenied {
        FsError::PathOutsideScope {
            path: target.requested_path.clone(),
        }
    } else {
        FsError::Io {
            operation,
            path: target.requested_path.clone(),
            source,
        }
    }
}

fn is_normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| !matches!(component, Component::CurDir | Component::ParentDir))
}

fn relative_to(path: &Path, root: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let path_wsl = wsl_unc_components(path);
        let root_wsl = wsl_unc_components(root);
        match (path_wsl, root_wsl) {
            (Some((path_distro, path_tail)), Some((root_distro, root_tail))) => {
                if !path_distro.eq_ignore_ascii_case(&root_distro)
                    || path_tail.len() < root_tail.len()
                    || !path_tail
                        .iter()
                        .zip(&root_tail)
                        .all(|(left, right)| left == right)
                {
                    return None;
                }
                return Some(path_tail[root_tail.len()..].iter().collect());
            }
            (Some(_), None) | (None, Some(_)) => return None,
            (None, None) => {}
        }
        let path_components = path.components().collect::<Vec<_>>();
        let root_components = root.components().collect::<Vec<_>>();
        if path_components.len() < root_components.len()
            || !path_components
                .iter()
                .zip(&root_components)
                .all(|(left, right)| {
                    left.as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
                })
        {
            return None;
        }
        Some(path_components[root_components.len()..].iter().collect())
    }
    #[cfg(not(windows))]
    {
        path.strip_prefix(root).ok().map(Path::to_path_buf)
    }
}

#[cfg(windows)]
fn wsl_unc_components(path: &Path) -> Option<(String, Vec<std::ffi::OsString>)> {
    let mut components = path.components();
    let Component::Prefix(prefix) = components.next()? else {
        return None;
    };
    let (server, distro) = match prefix.kind() {
        Prefix::UNC(server, distro) | Prefix::VerbatimUNC(server, distro) => (server, distro),
        _ => return None,
    };
    let server = server.to_string_lossy();
    if !server.eq_ignore_ascii_case("wsl.localhost") && !server.eq_ignore_ascii_case("wsl$") {
        return None;
    }
    let distro = distro.to_string_lossy().into_owned();
    let tail = components
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        })
        .collect();
    Some((distro, tail))
}

fn path_kind(file_type: FileType) -> PathKind {
    if file_type.is_symlink() {
        PathKind::Symlink
    } else if file_type.is_dir() {
        PathKind::Directory
    } else if file_type.is_file() {
        PathKind::File
    } else {
        PathKind::Other
    }
}

fn modified_time_millis(metadata: &Metadata, path: &Path) -> Result<f64, FsError> {
    let modified = metadata.modified().map_err(|source| FsError::Io {
        operation: FsOperation::Stat,
        path: path.to_path_buf(),
        source,
    })?;
    Ok(match modified.into_std().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64() * 1_000.0,
        Err(error) => -(error.duration().as_secs_f64() * 1_000.0),
    })
}

fn compare_directory_entries(left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
    let left_is_directory = left.kind == PathKind::Directory;
    let right_is_directory = right.kind == PathKind::Directory;
    right_is_directory
        .cmp(&left_is_directory)
        .then_with(|| natural_compare(&left.name, &right.name))
        .then_with(|| left.name.cmp(&right.name))
}

fn natural_compare(left: &str, right: &str) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left.len() && right_index < right.len() {
        let left_character = left[left_index..]
            .chars()
            .next()
            .expect("index is inside string");
        let right_character = right[right_index..]
            .chars()
            .next()
            .expect("index is inside string");
        if left_character.is_ascii_digit() && right_character.is_ascii_digit() {
            let left_end = digit_run_end(&left, left_index);
            let right_end = digit_run_end(&right, right_index);
            let ordering =
                compare_digit_runs(&left[left_index..left_end], &right[right_index..right_end]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }
        let ordering = left_character.cmp(&right_character);
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_index += left_character.len_utf8();
        right_index += right_character.len_utf8();
    }
    match (left_index == left.len(), right_index == right.len()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => unreachable!("comparison stops only when one string is exhausted"),
    }
}

fn digit_run_end(value: &str, start: usize) -> usize {
    start
        + value[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .map(char::len_utf8)
            .sum::<usize>()
}

fn compare_digit_runs(left: &str, right: &str) -> Ordering {
    let left_trimmed = left.trim_start_matches('0');
    let right_trimmed = right.trim_start_matches('0');
    let left_value = if left_trimmed.is_empty() {
        "0"
    } else {
        left_trimmed
    };
    let right_value = if right_trimmed.is_empty() {
        "0"
    } else {
        right_trimmed
    };
    left_value
        .len()
        .cmp(&right_value.len())
        .then_with(|| left_value.cmp(right_value))
}

#[cfg(test)]
mod tests {
    use super::{RegisteredRoot, ScopeId, absolute_link_target_inside_root, relative_to};
    use cap_std::{ambient_authority, fs::Dir};
    #[cfg(windows)]
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn absolute_link_targets_with_parent_components_are_rejected() {
        let root_path = std::env::temp_dir();
        let root = RegisteredRoot {
            scope_id: ScopeId(1),
            requested_path: root_path.clone(),
            canonical_path: std::fs::canonicalize(&root_path).unwrap(),
            directory: Arc::new(Dir::open_ambient_dir(&root_path, ambient_authority()).unwrap()),
        };
        let target = root_path.join(PathBuf::from("a").join("..").join("target"));

        assert_eq!(absolute_link_target_inside_root(&root, &target), None);
    }

    #[cfg(windows)]
    #[test]
    fn wsl_share_and_distro_are_case_insensitive_but_linux_tail_is_not() {
        let root = Path::new(r"\\wsl.localhost\Ubuntu\home\Alice\repo");
        assert_eq!(
            relative_to(
                Path::new(r"\\WSL.LOCALHOST\ubuntu\home\Alice\repo\src"),
                root
            )
            .as_deref(),
            Some(Path::new("src"))
        );
        assert!(
            relative_to(
                Path::new(r"\\wsl.localhost\Ubuntu\home\alice\repo\src"),
                root
            )
            .is_none()
        );
    }
}
