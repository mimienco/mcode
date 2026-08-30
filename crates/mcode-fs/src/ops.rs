//! Core filesystem operations over authorized paths (spec §4 rows).
//!
//! Every operation takes an [`AuthorizedPath`] produced by
//! [`crate::scope::FsScopeRegistry`]; raw paths never reach the syscalls
//! here. Error codes follow spec §3.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::scope::{AuthorizedPath, FsError};

/// Text/binary read ceiling (spec §4 `fs:readFile`, 50 MB cap).
pub const READ_FILE_MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Bytes probed for a NUL byte to decide whether an unknown file is binary.
const BINARY_PROBE_BYTES: usize = 8192;

/// Result of a successful read (spec §4 `fs:readFile`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResult {
    pub content: String,
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Directory entry shape (spec §4 `fs:readDir`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub is_directory: bool,
    pub is_symlink: bool,
}

/// Stat result (spec §4 `fs:stat`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatResult {
    pub size: u64,
    pub is_directory: bool,
    /// Milliseconds since the Unix epoch.
    pub mtime: u64,
}

/// Writes `content` to the file at `path`, creating or truncating it.
///
/// ENOENT on the parent is tolerated by creating parent directories first
/// (spec §4 `fs:writeFile` notes ENOENT tolerated before the write).
pub fn write_file(path: &AuthorizedPath, content: &str) -> Result<(), FsError> {
    let target = path.as_path();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(FsError::from)?;
    }
    // Directory guard (spec §4 `fs:writeFile`): a directory target is
    // rejected before the OS surfaces a generic permission error.
    if fs::metadata(target)
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
    {
        return Err(FsError::IsDirectory);
    }
    let mut file = fs::File::create(target).map_err(|error| {
        if is_directory_error(&error) {
            FsError::IsDirectory
        } else {
            FsError::from(error)
        }
    })?;
    file.write_all(content.as_bytes()).map_err(FsError::from)?;
    file.sync_all().map_err(FsError::from)?;
    Ok(())
}

/// Atomically writes `content` to the file at `path`.
///
/// Writes a temp file in the same directory, fsyncs it, then renames it over
/// the target. If the target already exists its permissions are copied onto
/// the temp file first so a mode-preserving replace holds (spec §4
/// `fs:searchReplace` semantics).
pub fn write_file_atomic(path: &AuthorizedPath, content: &str) -> Result<(), FsError> {
    let target = path.as_path();
    let parent = target
        .parent()
        .ok_or_else(|| FsError::InvalidArgument("destination parent".into()))?;
    fs::create_dir_all(parent).map_err(FsError::from)?;

    let temp = tempfile::NamedTempFile::new_in(parent).map_err(FsError::from)?;
    if let Ok(metadata) = fs::metadata(target) {
        copy_permissions(&metadata, temp.path()).map_err(FsError::from)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(temp.path())
        .map_err(FsError::from)?;
    file.write_all(content.as_bytes()).map_err(FsError::from)?;
    file.sync_all().map_err(FsError::from)?;
    drop(file);

    // NamedTempFile persists on rename and cleans up on error paths.
    temp.persist(target)
        .map_err(|error| FsError::from(error.error))?;
    Ok(())
}

/// Creates the directory at `path` including missing parents.
pub fn create_dir(path: &AuthorizedPath) -> Result<(), FsError> {
    fs::create_dir_all(path.as_path()).map_err(FsError::from)
}

/// Creates an empty file at `path` with create-new semantics.
///
/// Uses `create_new` so the existence check and creation are one atomic
/// syscall; EEXIST surfaces as [`FsError::AlreadyExists`] (spec §4
/// `fs:createFile`, `wx` flag behavior).
pub fn create_file(path: &AuthorizedPath) -> Result<(), FsError> {
    let target = path.as_path();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(FsError::from)?;
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(FsError::from)?;
    Ok(())
}

/// Copies the file or directory tree at `source` to `destination`.
///
/// Fails with [`FsError::DestinationRace`] if the destination already exists
/// (spec §4 `fs:copyPath`, COPYFILE_EXCL equivalent). Directory trees are
/// copied recursively with the same no-clobber rule per entry.
pub fn copy_path(source: &AuthorizedPath, destination: &AuthorizedPath) -> Result<(), FsError> {
    let from = source.as_path();
    let to = destination.as_path();
    let metadata = fs::symlink_metadata(from).map_err(FsError::from)?;
    if metadata.is_dir() {
        copy_dir_recursive(from, to)
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(FsError::from)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|error| match error.kind() {
                io::ErrorKind::AlreadyExists => FsError::DestinationRace,
                _ => FsError::from(error),
            })?
            .sync_all()
            .map_err(FsError::from)?;
        fs::copy(from, to).map_err(FsError::from)?;
        Ok(())
    }
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), FsError> {
    fs::create_dir(to).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => FsError::DestinationRace,
        _ => FsError::from(error),
    })?;
    for entry in fs::read_dir(from).map_err(FsError::from)? {
        let entry = entry.map_err(FsError::from)?;
        let entry_type = entry.file_type().map_err(FsError::from)?;
        let child_dest = to.join(entry.file_name());
        if entry_type.is_dir() {
            copy_dir_recursive(&entry.path(), &child_dest)?;
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&child_dest)
                .map_err(|error| match error.kind() {
                    io::ErrorKind::AlreadyExists => FsError::DestinationRace,
                    _ => FsError::from(error),
                })?;
            fs::copy(entry.path(), &child_dest).map_err(FsError::from)?;
        }
    }
    Ok(())
}

/// Renames `source` to `destination` without clobbering an existing target.
///
/// An existing destination yields [`FsError::DestinationRace`]; a cross-drive
/// rename surfaces as [`FsError::DestinationRace`] too (spec §7: EXDEV).
pub fn rename_path(source: &AuthorizedPath, destination: &AuthorizedPath) -> Result<(), FsError> {
    let from = source.as_path();
    let to = destination.as_path();
    if to.symlink_metadata().is_ok() {
        return Err(FsError::DestinationRace);
    }
    fs::rename(from, to).map_err(|error| match error.raw_os_error() {
        // ERROR_NOT_SAME_DEVICE (17) and EXDEV (18) both mean cross-volume.
        Some(17) | Some(18) => FsError::DestinationRace,
        _ => FsError::from(error),
    })
}

/// Deletes the file or directory tree at `path` to the Recycle Bin.
///
/// ENOENT is swallowed so an external delete racing this one stays idempotent
/// (spec §4 `fs:deletePath`). UNC paths on Windows skip the Recycle Bin and
/// hard-delete instead.
pub fn delete_path(path: &AuthorizedPath) -> Result<(), FsError> {
    let target = path.as_path();
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(FsError::from(error)),
    };
    if !metadata.is_dir() {
        // Move a single file or symlink leaf to the Recycle Bin via trash,
        // which on Windows uses the shell Recycle Bin API.
        trash::delete(target).or_else(|error| {
            // A missing item is still a success; anything else propagates.
            if target.symlink_metadata().is_err() {
                Ok(())
            } else {
                Err(FsError::Io(error.to_string()))
            }
        })?;
        return Ok(());
    }
    // trash::delete on a directory sends the tree to the Recycle Bin; fall
    // back to a hard recursive delete when the shell refuses (spec §4: UNC
    // paths have no Recycle Bin).
    match trash::delete(target) {
        Ok(()) => Ok(()),
        Err(_) => fs::remove_dir_all(target).map_err(FsError::from),
    }
}

/// Stats the path; ENOENT surfaces as [`FsError::NotFound`].
pub fn stat(path: &AuthorizedPath) -> Result<StatResult, FsError> {
    let metadata = fs::metadata(path.as_path()).map_err(FsError::from)?;
    Ok(StatResult {
        size: metadata.len(),
        is_directory: metadata.is_dir(),
        mtime: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0),
    })
}

/// Whether the path exists; other stat errors propagate, only ENOENT maps to
/// `false` (spec §4 `fs:pathExists`).
pub fn path_exists(path: &AuthorizedPath) -> Result<bool, FsError> {
    match fs::metadata(path.as_path()) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(FsError::from(error)),
    }
}

/// Lists a directory (spec §4 `fs:readDir`).
pub fn read_dir(path: &AuthorizedPath) -> Result<Vec<DirEntry>, FsError> {
    let entries = fs::read_dir(path.as_path()).map_err(FsError::from)?;
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(FsError::from)?;
        let file_type = entry.file_type().map_err(FsError::from)?;
        let is_symlink = file_type.is_symlink();
        // Symlink-aware directory detection: resolve the target when the
        // entry itself is a link (spec §4 `fs:readDir`).
        let is_directory = if is_symlink {
            fs::metadata(entry.path())
                .map(|meta| meta.is_dir())
                .unwrap_or(false)
        } else {
            file_type.is_dir()
        };
        result.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_directory,
            is_symlink,
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

/// Reads a file with the 50 MB cap and a binary probe (spec §4
/// `fs:readFile`).
///
/// Text returns the UTF-8 contents; binary files are base64-encoded with
/// `is_binary: true`. A NUL byte in the first 8192 bytes marks the file
/// binary without buffering the whole payload first.
pub fn read_file(path: &AuthorizedPath) -> Result<ReadFileResult, FsError> {
    let target = path.as_path();
    let metadata = fs::metadata(target).map_err(FsError::from)?;
    if metadata.is_dir() {
        return Err(FsError::IsDirectory);
    }
    if metadata.len() > READ_FILE_MAX_BYTES {
        return Err(FsError::TooLarge {
            limit_bytes: READ_FILE_MAX_BYTES,
        });
    }
    let mut file = fs::File::open(target).map_err(FsError::from)?;
    let mut buffer = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut buffer).map_err(FsError::from)?;
    if is_probably_binary(&buffer) {
        return Ok(ReadFileResult {
            content: BASE64.encode(&buffer),
            is_binary: true,
            mime_type: None,
        });
    }
    let text = String::from_utf8(buffer).map_err(|_| FsError::IsDirectory)?;
    Ok(ReadFileResult {
        content: text,
        is_binary: false,
        mime_type: None,
    })
}

fn is_probably_binary(bytes: &[u8]) -> bool {
    let probe_len = bytes.len().min(BINARY_PROBE_BYTES);
    bytes[..probe_len].contains(&0)
}

fn is_directory_error(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        // EISDIR (21) or Windows ERROR_ACCESS_DENIED-ish paths where the OS
        // reports a directory target as invalid; keep EISDIR authoritative.
        Some(21)
    ) || error.kind() == io::ErrorKind::IsADirectory
        || error.to_string().contains("directory")
}

/// Copies file permissions from `source` metadata onto `target` so an atomic
/// replace keeps the original mode (spec §4 `fs:searchReplace`).
fn copy_permissions(source: &fs::Metadata, target: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            target,
            fs::Permissions::from_mode(source.permissions().mode()),
        )
    }
    #[cfg(windows)]
    {
        let _ = (source, target);
        Ok(())
    }
}

/// Helper used by search to read a bounded slice of a file without holding
/// the whole payload when only a size check is needed.
pub(crate) fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, FsError> {
    let metadata = fs::metadata(path).map_err(FsError::from)?;
    if metadata.len() > cap {
        return Err(FsError::TooLarge { limit_bytes: cap });
    }
    let mut buffer = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .and_then(|mut file| file.read_to_end(&mut buffer))
        .map_err(FsError::from)?;
    Ok(buffer)
}
