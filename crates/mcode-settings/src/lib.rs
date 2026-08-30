use atomic_write_file::AtomicWriteFile;
use mcode_platform::{DesktopWindowPreferences, WindowBounds};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Value};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod workspace_session_bindings;
mod workspace_session_contract;
mod workspace_session_reconciliation;
mod workspace_session_retirement_layouts;
mod workspace_session_retirements;
mod workspace_session_topology;

pub const SHUTDOWN_CHECKPOINT_VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownSession {
    pub state: WorkspaceSessionState,
    pub host_id: Option<ExecutionHostId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExecutionHostId(String);

impl ExecutionHostId {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into().trim().to_owned();
        if is_execution_host_id(&value) {
            Ok(Self(value))
        } else {
            Err("execution host must identify local, SSH, or runtime execution".into())
        }
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for ExecutionHostId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorkspaceSessionState(Map<String, Value>);

impl WorkspaceSessionState {
    fn into_value(self) -> Value {
        Value::Object(self.0)
    }
}

impl<'de> Deserialize<'de> for WorkspaceSessionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = Map::<String, Value>::deserialize(deserializer)?;
        workspace_session_contract::validate_workspace_session_state(&fields)
            .map_err(de::Error::custom)?;
        Ok(Self(fields))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum TopLevelView {
    Terminal,
    Settings,
    Tasks,
    Activity,
    Automations,
    Space,
    Skills,
    Artifacts,
    Mobile,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedUiPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    active_view: Option<TopLevelView>,
}

impl PersistedUiPatch {
    fn into_map(self) -> Map<String, Value> {
        match self.active_view {
            Some(active_view) => Map::from_iter([(
                "activeView".into(),
                serde_json::to_value(active_view)
                    .expect("top-level view serialization is infallible"),
            )]),
            None => Map::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownCheckpoint {
    pub version: u32,
    pub sessions: Vec<ShutdownSession>,
    pub ui: PersistedUiPatch,
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
    SettingsTooLarge(u64),
    Parse(String),
    PersistedSectionMustBeObject(&'static str),
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
            Self::PersistedSectionMustBeObject(_) => "settings_shape_invalid",
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
            Self::PersistedSectionMustBeObject(section) => {
                write!(formatter, "persisted {section} settings must be an object")
            }
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

#[derive(Clone, Debug)]
pub struct SettingsService {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl SettingsService {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn desktop_window_preferences(
        &self,
    ) -> Result<DesktopWindowPreferences, SettingsServiceError> {
        let _read_guard = self
            .write_lock
            .lock()
            .map_err(|_| SettingsServiceError::LockPoisoned)?;
        let persisted = self.read_persisted()?;
        let ui = persisted.get("ui").and_then(Value::as_object);
        let settings = persisted.get("settings").and_then(Value::as_object);

        Ok(DesktopWindowPreferences {
            bounds: ui
                .and_then(|ui| ui.get("windowBounds"))
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok()),
            maximized: ui
                .and_then(|ui| ui.get("windowMaximized"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            minimize_to_tray_on_close: settings
                .and_then(|settings| settings.get("minimizeToTrayOnClose"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub fn persist_window_placement(
        &self,
        bounds: Option<WindowBounds>,
        maximized: bool,
    ) -> Result<(), SettingsServiceError> {
        self.update_persisted(|root| {
            let ui = object_section(root, "ui")?;
            if let Some(bounds) = bounds {
                ui.insert(
                    "windowBounds".into(),
                    serde_json::to_value(bounds)
                        .map_err(|error| SettingsServiceError::Serialize(error.to_string()))?,
                );
            }
            ui.insert("windowMaximized".into(), Value::Bool(maximized));
            Ok(())
        })
    }

    pub fn set_minimize_to_tray_on_close(&self, enabled: bool) -> Result<(), SettingsServiceError> {
        self.update_persisted(|root| {
            object_section(root, "settings")?
                .insert("minimizeToTrayOnClose".into(), Value::Bool(enabled));
            Ok(())
        })
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

        match fs::metadata(&self.path) {
            Ok(metadata) => enforce_size_limit(metadata.len())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(SettingsServiceError::Read(error.to_string())),
        }
        let existing_bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => b"{}".to_vec(),
            Err(error) => return Err(SettingsServiceError::Read(error.to_string())),
        };
        enforce_size_limit(existing_bytes.len() as u64)?;
        let persisted = serde_json::from_slice(&existing_bytes)
            .map_err(|error| SettingsServiceError::Parse(error.to_string()))?;
        let merged = apply_shutdown_checkpoint(persisted, checkpoint)
            .map_err(SettingsServiceError::Checkpoint)?;
        let serialized = serde_json::to_vec_pretty(&merged)
            .map_err(|error| SettingsServiceError::Serialize(error.to_string()))?;
        enforce_size_limit(serialized.len() as u64)?;

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

    fn update_persisted(
        &self,
        update: impl FnOnce(&mut Map<String, Value>) -> Result<(), SettingsServiceError>,
    ) -> Result<(), SettingsServiceError> {
        let _write_guard = self
            .write_lock
            .lock()
            .map_err(|_| SettingsServiceError::LockPoisoned)?;
        let mut persisted = self.read_persisted()?;
        let root = persisted
            .as_object_mut()
            .ok_or(SettingsServiceError::PersistedSectionMustBeObject("root"))?;
        update(root)?;
        self.write_persisted(&persisted)
    }

    fn read_persisted(&self) -> Result<Value, SettingsServiceError> {
        match fs::metadata(&self.path) {
            Ok(metadata) => enforce_size_limit(metadata.len())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Value::Object(Map::new()));
            }
            Err(error) => return Err(SettingsServiceError::Read(error.to_string())),
        }
        let bytes =
            fs::read(&self.path).map_err(|error| SettingsServiceError::Read(error.to_string()))?;
        enforce_size_limit(bytes.len() as u64)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| SettingsServiceError::Parse(error.to_string()))
    }

    fn write_persisted(&self, persisted: &Value) -> Result<(), SettingsServiceError> {
        let parent = self.path.parent().ok_or_else(|| {
            SettingsServiceError::CreateDirectory("settings path has no parent".into())
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| SettingsServiceError::CreateDirectory(error.to_string()))?;
        let serialized = serde_json::to_vec_pretty(persisted)
            .map_err(|error| SettingsServiceError::Serialize(error.to_string()))?;
        enforce_size_limit(serialized.len() as u64)?;
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

fn object_section<'a>(
    root: &'a mut Map<String, Value>,
    section: &'static str,
) -> Result<&'a mut Map<String, Value>, SettingsServiceError> {
    root.entry(section)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or(SettingsServiceError::PersistedSectionMustBeObject(section))
}

fn enforce_size_limit(size: u64) -> Result<(), SettingsServiceError> {
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
    ui.extend(checkpoint.ui.into_map());

    for session in checkpoint.sessions {
        if let Some(host_id) = session.host_id {
            let host_id = host_id.into_string();
            let prior = root
                .get("workspaceSessionsByHostId")
                .and_then(Value::as_object)
                .and_then(|partitions| partitions.get(&host_id))
                .cloned();
            let mut state = session.state.into_value();
            workspace_session_reconciliation::reconcile_host_workspace_session(
                &mut state,
                prior.as_ref(),
            );
            let partitions = root
                .entry("workspaceSessionsByHostId")
                .or_insert_with(|| Value::Object(Map::new()));
            let partitions = partitions
                .as_object_mut()
                .ok_or(CheckpointError::PersistedHostSessionsMustBeObject)?;
            partitions.insert(host_id, state);
        } else {
            let prior = root.get("workspaceSession").cloned();
            let mut state = session.state.into_value();
            workspace_session_reconciliation::reconcile_local_workspace_session(
                &mut state,
                prior.as_ref(),
                root,
            );
            root.insert("workspaceSession".into(), state);
        }
    }

    Ok(persisted)
}

fn is_execution_host_id(value: &str) -> bool {
    value == "local"
        || value
            .strip_prefix("ssh:")
            .is_some_and(is_valid_uri_component)
        || value
            .strip_prefix("runtime:")
            .is_some_and(is_valid_uri_component)
}

fn is_valid_uri_component(value: &str) -> bool {
    decode_uri_component(value).is_some_and(|decoded| !decoded.is_empty())
}

fn decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex_value(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex_value(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
