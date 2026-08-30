use crate::scoped::{
    enforce_file_size, file_identity, is_normalized_absolute, modified_time_millis, relative_to,
};
use crate::{
    BoundedFileRead, BoundedFileSnapshot, DirectoryEntry, FsError, FsOperation, PathKind,
    PathMetadata, ScopedFilesystem,
};
use cap_std::fs::File;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A short-lived capability produced by a trusted native path source.
///
/// Files are pinned to the handle opened during authorization and are read-only.
/// Directories delegate to a capability-relative scoped filesystem rooted at the
/// captured directory, which grants the complete subtree without allowing escape.
#[derive(Clone, Debug)]
pub struct ExactPathCapability {
    requested_path: PathBuf,
    canonical_path: PathBuf,
    target: ExactPathTarget,
}

#[derive(Clone, Debug)]
enum ExactPathTarget {
    File(Arc<Mutex<File>>),
    Directory(ScopedFilesystem),
}

impl ExactPathCapability {
    pub fn authorize(target_path: impl AsRef<Path>) -> Result<Self, FsError> {
        let requested_path = target_path.as_ref().to_path_buf();
        if !is_normalized_absolute(&requested_path) {
            return Err(FsError::InvalidTargetPath {
                path: requested_path,
            });
        }

        let metadata = fs::metadata(&requested_path).map_err(|source| FsError::Io {
            operation: FsOperation::ResolvePath,
            path: requested_path.clone(),
            source,
        })?;

        let (canonical_path, target) = if metadata.is_dir() {
            let filesystem = ScopedFilesystem::new();
            filesystem.register_root(&requested_path)?;
            let canonical_path =
                fs::canonicalize(&requested_path).map_err(|source| FsError::Io {
                    operation: FsOperation::ResolvePath,
                    path: requested_path.clone(),
                    source,
                })?;
            (canonical_path, ExactPathTarget::Directory(filesystem))
        } else if metadata.is_file() {
            // Open the native-provided path exactly once. Every later access
            // uses this pinned handle, so retargeting the leaf cannot expose a
            // sibling between path resolution and the operation.
            let file = fs::File::open(&requested_path).map_err(|source| FsError::Io {
                operation: FsOperation::ResolvePath,
                path: requested_path.clone(),
                source,
            })?;
            let captured_metadata = file.metadata().map_err(|source| FsError::Io {
                operation: FsOperation::ResolvePath,
                path: requested_path.clone(),
                source,
            })?;
            if !captured_metadata.is_file() {
                return Err(FsError::Io {
                    operation: FsOperation::ResolvePath,
                    path: requested_path,
                    source: io::Error::new(io::ErrorKind::InvalidInput, "unsupported path kind"),
                });
            }
            let canonical_path =
                fs::canonicalize(&requested_path).map_err(|source| FsError::Io {
                    operation: FsOperation::ResolvePath,
                    path: requested_path.clone(),
                    source,
                })?;
            (
                canonical_path,
                ExactPathTarget::File(Arc::new(Mutex::new(File::from_std(file)))),
            )
        } else {
            return Err(FsError::Io {
                operation: FsOperation::ResolvePath,
                path: requested_path,
                source: io::Error::new(io::ErrorKind::InvalidInput, "unsupported path kind"),
            });
        };

        Ok(Self {
            requested_path,
            canonical_path,
            target,
        })
    }

    pub fn matches(&self, target_path: &Path) -> bool {
        self.map_target(target_path).is_some()
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn read_dir(&self, target_path: &Path) -> Result<Vec<DirectoryEntry>, FsError> {
        match (&self.target, self.map_target(target_path)) {
            (ExactPathTarget::Directory(filesystem), Some(mapped_path)) => {
                filesystem.read_dir(&mapped_path)
            }
            (ExactPathTarget::File(_), Some(_)) => Err(FsError::Io {
                operation: FsOperation::ReadDirectory,
                path: target_path.to_path_buf(),
                source: io::Error::new(io::ErrorKind::NotADirectory, "target is not a directory"),
            }),
            (_, None) => Err(outside_scope(target_path)),
        }
    }

    pub fn stat(&self, target_path: &Path) -> Result<PathMetadata, FsError> {
        match (&self.target, self.map_target(target_path)) {
            (ExactPathTarget::Directory(filesystem), Some(mapped_path)) => {
                filesystem.stat(&mapped_path)
            }
            (ExactPathTarget::File(file), Some(_)) => {
                let file = file.lock().map_err(|_| FsError::StateUnavailable)?;
                let metadata = file.metadata().map_err(|source| FsError::Io {
                    operation: FsOperation::Stat,
                    path: target_path.to_path_buf(),
                    source,
                })?;
                Ok(PathMetadata {
                    kind: PathKind::File,
                    size: metadata.len(),
                    modified_time_millis: modified_time_millis(&metadata, target_path)?,
                })
            }
            (_, None) => Err(outside_scope(target_path)),
        }
    }

    pub fn path_exists(&self, target_path: &Path) -> Result<bool, FsError> {
        match (&self.target, self.map_target(target_path)) {
            (ExactPathTarget::Directory(filesystem), Some(mapped_path)) => {
                filesystem.path_exists(&mapped_path)
            }
            (ExactPathTarget::File(_), Some(_)) => Ok(true),
            (_, None) => Err(outside_scope(target_path)),
        }
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
        match (&self.target, self.map_target(target_path)) {
            (ExactPathTarget::Directory(filesystem), Some(mapped_path)) => {
                filesystem.read_file_snapshot_bounded(&mapped_path, max_bytes, binary_probe_bytes)
            }
            (ExactPathTarget::File(file), Some(_)) => {
                let mut file = file.lock().map_err(|_| FsError::StateUnavailable)?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|source| FsError::Io {
                        operation: FsOperation::ReadFile,
                        path: target_path.to_path_buf(),
                        source,
                    })?;
                let metadata = file.metadata().map_err(|source| FsError::Io {
                    operation: FsOperation::ReadFile,
                    path: target_path.to_path_buf(),
                    source,
                })?;
                enforce_file_size(target_path, metadata.len(), max_bytes)?;
                let identity = file_identity(&metadata);

                let probe_limit = binary_probe_bytes.unwrap_or(0).min(max_bytes);
                let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes as u64) as usize);
                if probe_limit > 0 {
                    (&mut *file)
                        .take(probe_limit as u64)
                        .read_to_end(&mut bytes)
                        .map_err(|source| FsError::Io {
                            operation: FsOperation::ReadFile,
                            path: target_path.to_path_buf(),
                            source,
                        })?;
                    if bytes.contains(&0) {
                        return Ok(BoundedFileSnapshot {
                            read: BoundedFileRead::BinaryPrefix,
                            file_identity: identity,
                        });
                    }
                }

                let remaining_with_sentinel =
                    max_bytes.saturating_sub(bytes.len()).saturating_add(1);
                (&mut *file)
                    .take(remaining_with_sentinel as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|source| FsError::Io {
                        operation: FsOperation::ReadFile,
                        path: target_path.to_path_buf(),
                        source,
                    })?;
                enforce_file_size(target_path, bytes.len() as u64, max_bytes)?;
                Ok(BoundedFileSnapshot {
                    read: BoundedFileRead::Bytes(bytes),
                    file_identity: identity,
                })
            }
            (_, None) => Err(outside_scope(target_path)),
        }
    }

    pub fn write_file_atomic(
        &self,
        target_path: &Path,
        contents: &[u8],
        max_bytes: usize,
    ) -> Result<(), FsError> {
        match (&self.target, self.map_target(target_path)) {
            (ExactPathTarget::Directory(filesystem), Some(mapped_path)) => {
                filesystem.write_file_atomic(&mapped_path, contents, max_bytes)
            }
            (ExactPathTarget::File(_), Some(_)) => Err(FsError::ExternalWriteUnsupported {
                path: target_path.to_path_buf(),
            }),
            (_, None) => Err(outside_scope(target_path)),
        }
    }

    fn map_target(&self, target_path: &Path) -> Option<PathBuf> {
        if !is_normalized_absolute(target_path) {
            return None;
        }
        match self.target {
            ExactPathTarget::File(_) => paths_equal(target_path, &self.requested_path)
                .then(|| self.canonical_path.clone())
                .or_else(|| {
                    paths_equal(target_path, &self.canonical_path)
                        .then(|| self.canonical_path.clone())
                }),
            ExactPathTarget::Directory(_) => relative_to(target_path, &self.requested_path)
                .or_else(|| relative_to(target_path, &self.canonical_path))
                .map(|relative| self.canonical_path.join(relative)),
        }
    }
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    relative_to(left, right).is_some_and(|relative| relative.as_os_str().is_empty())
        && relative_to(right, left).is_some_and(|relative| relative.as_os_str().is_empty())
}

fn outside_scope(path: &Path) -> FsError {
    FsError::PathOutsideScope {
        path: path.to_path_buf(),
    }
}
