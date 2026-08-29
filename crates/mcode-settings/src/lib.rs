use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SHUTDOWN_CHECKPOINT_VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownSession {
    pub state: Value,
    pub host_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownCheckpoint {
    pub version: u32,
    pub sessions: Vec<ShutdownSession>,
    pub ui: Map<String, Value>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CheckpointError {
    UnsupportedVersion(u32),
    PersistedStateMustBeObject,
    PersistedUiMustBeObject,
    PersistedHostSessionsMustBeObject,
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported shutdown checkpoint version {version}"
                )
            }
            Self::PersistedStateMustBeObject => {
                formatter.write_str("persisted settings root must be an object")
            }
            Self::PersistedUiMustBeObject => {
                formatter.write_str("persisted UI state must be an object")
            }
            Self::PersistedHostSessionsMustBeObject => {
                formatter.write_str("persisted host sessions must be an object")
            }
        }
    }
}

impl std::error::Error for CheckpointError {}

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsServiceError {
    LockPoisoned,
    CreateDirectory(String),
    Read(String),
    SettingsTooLarge(usize),
    Parse(String),
    Checkpoint(CheckpointError),
    Serialize(String),
    OpenAtomicFile(String),
    WriteAtomicFile(String),
    SyncAtomicFile(String),
    CommitAtomicFile(String),
}

impl SettingsServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::LockPoisoned => "settings_lock_poisoned",
            Self::CreateDirectory(_) => "settings_create_directory_failed",
            Self::Read(_) => "settings_read_failed",
            Self::SettingsTooLarge(_) => "settings_too_large",
            Self::Parse(_) => "settings_parse_failed",
            Self::Checkpoint(_) => "shutdown_checkpoint_invalid",
            Self::Serialize(_) => "settings_serialize_failed",
            Self::OpenAtomicFile(_) => "settings_atomic_open_failed",
            Self::WriteAtomicFile(_) => "settings_atomic_write_failed",
            Self::SyncAtomicFile(_) => "settings_atomic_sync_failed",
            Self::CommitAtomicFile(_) => "settings_atomic_commit_failed",
        }
    }
}

impl fmt::Display for SettingsServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockPoisoned => formatter.write_str("settings write lock is unavailable"),
            Self::CreateDirectory(message) => {
                write!(formatter, "failed to create settings directory: {message}")
            }
            Self::Read(message) => write!(formatter, "failed to read settings: {message}"),
            Self::SettingsTooLarge(size) => write!(
                formatter,
                "settings payload is {size} bytes, exceeding the {MAX_SETTINGS_BYTES}-byte limit"
            ),
            Self::Parse(message) => write!(formatter, "failed to parse settings: {message}"),
            Self::Checkpoint(error) => write!(formatter, "invalid shutdown checkpoint: {error}"),
            Self::Serialize(message) => {
                write!(formatter, "failed to serialize settings: {message}")
            }
            Self::OpenAtomicFile(message) => {
                write!(formatter, "failed to open atomic settings file: {message}")
            }
            Self::WriteAtomicFile(message) => {
                write!(formatter, "failed to write atomic settings file: {message}")
            }
            Self::SyncAtomicFile(message) => {
                write!(formatter, "failed to sync atomic settings file: {message}")
            }
            Self::CommitAtomicFile(message) => {
                write!(
                    formatter,
                    "failed to commit atomic settings file: {message}"
                )
            }
        }
    }
}

impl std::error::Error for SettingsServiceError {}

#[derive(Debug)]
pub struct SettingsService {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl SettingsService {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn stage_shutdown_checkpoint(
        &self,
        checkpoint: ShutdownCheckpoint,
    ) -> Result<(), SettingsServiceError> {
        let _write_guard = self
            .write_lock
            .lock()
            .map_err(|_| SettingsServiceError::LockPoisoned)?;
        let parent = self.path.parent().ok_or_else(|| {
            SettingsServiceError::CreateDirectory("settings path has no parent".into())
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| SettingsServiceError::CreateDirectory(error.to_string()))?;

        let existing_bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => b"{}".to_vec(),
            Err(error) => return Err(SettingsServiceError::Read(error.to_string())),
        };
        enforce_size_limit(existing_bytes.len())?;
        let persisted = serde_json::from_slice(&existing_bytes)
            .map_err(|error| SettingsServiceError::Parse(error.to_string()))?;
        let merged = apply_shutdown_checkpoint(persisted, checkpoint)
            .map_err(SettingsServiceError::Checkpoint)?;
        let serialized = serde_json::to_vec_pretty(&merged)
            .map_err(|error| SettingsServiceError::Serialize(error.to_string()))?;
        enforce_size_limit(serialized.len())?;

        let mut atomic_file = AtomicWriteFile::open(&self.path)
            .map_err(|error| SettingsServiceError::OpenAtomicFile(error.to_string()))?;
        atomic_file
            .write_all(&serialized)
            .map_err(|error| SettingsServiceError::WriteAtomicFile(error.to_string()))?;
        atomic_file
            .sync_all()
            .map_err(|error| SettingsServiceError::SyncAtomicFile(error.to_string()))?;
        atomic_file
            .commit()
            .map_err(|error| SettingsServiceError::CommitAtomicFile(error.to_string()))
    }
}

fn enforce_size_limit(size: usize) -> Result<(), SettingsServiceError> {
    if size > MAX_SETTINGS_BYTES {
        Err(SettingsServiceError::SettingsTooLarge(size))
    } else {
        Ok(())
    }
}

pub fn apply_shutdown_checkpoint(
    mut persisted: Value,
    checkpoint: ShutdownCheckpoint,
) -> Result<Value, CheckpointError> {
    if checkpoint.version != SHUTDOWN_CHECKPOINT_VERSION {
        return Err(CheckpointError::UnsupportedVersion(checkpoint.version));
    }
    let root = persisted
        .as_object_mut()
        .ok_or(CheckpointError::PersistedStateMustBeObject)?;
    let ui = root
        .entry("ui")
        .or_insert_with(|| Value::Object(Map::new()));
    let ui = ui
        .as_object_mut()
        .ok_or(CheckpointError::PersistedUiMustBeObject)?;
    ui.extend(checkpoint.ui);

    for session in checkpoint.sessions {
        if let Some(host_id) = session.host_id {
            let partitions = root
                .entry("workspaceSessionsByHostId")
                .or_insert_with(|| Value::Object(Map::new()));
            let partitions = partitions
                .as_object_mut()
                .ok_or(CheckpointError::PersistedHostSessionsMustBeObject)?;
            partitions.insert(host_id, session.state);
        } else {
            root.insert("workspaceSession".into(), session.state);
        }
    }

    Ok(persisted)
}
