use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Value};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
        validate_workspace_session_state(&fields).map_err(de::Error::custom)?;
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
            let partitions = root
                .entry("workspaceSessionsByHostId")
                .or_insert_with(|| Value::Object(Map::new()));
            let partitions = partitions
                .as_object_mut()
                .ok_or(CheckpointError::PersistedHostSessionsMustBeObject)?;
            partitions.insert(host_id.into_string(), session.state.into_value());
        } else {
            root.insert("workspaceSession".into(), session.state.into_value());
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
    if value.is_empty() {
        return false;
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(high) = bytes.get(index + 1).and_then(|byte| hex_value(*byte)) else {
                return false;
            };
            let Some(low) = bytes.get(index + 2).and_then(|byte| hex_value(*byte)) else {
                return false;
            };
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).is_ok_and(|decoded| !decoded.is_empty())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn validate_workspace_session_state(fields: &Map<String, Value>) -> Result<(), String> {
    for required in [
        "activeRepoId",
        "activeWorktreeId",
        "activeTabId",
        "tabsByWorktree",
        "terminalLayoutsByTabId",
    ] {
        if !fields.contains_key(required) {
            return Err(format!("workspace session is missing {required}"));
        }
    }

    for (name, value) in fields {
        let valid = match name.as_str() {
            "activeRepoId" | "activeWorktreeId" | "activeTabId" => is_nullable_string(value),
            "activeWorkspaceKey" => value.is_null() || value.as_str().is_some_and(is_workspace_key),
            "activeWorkspaceExecutionHostId" => {
                value.is_null() || value.as_str().is_some_and(is_execution_host_id)
            }
            "tabsByWorktree" => {
                is_record_of(value, |entry| is_array_of(entry, validate_terminal_tab))
            }
            "terminalLayoutsByTabId" => is_record_of_with_key(value, |tab_id, entry| {
                is_terminal_tab_id_str(tab_id) && validate_terminal_layout_snapshot(entry)
            }),
            "openFilesByWorktree" => is_record_of(value, |entry| {
                is_array_of(entry, validate_persisted_open_file)
            }),
            "browserTabsByWorktree" => is_record_of(value, |entry| {
                is_array_of(entry, validate_browser_workspace)
            }),
            "browserPagesByWorkspace" => {
                is_record_of(value, |entry| is_array_of(entry, validate_browser_page))
            }
            "unifiedTabs" => is_record_of(value, |entry| is_array_of(entry, validate_unified_tab)),
            "tabGroups" => is_record_of(value, |entry| is_array_of(entry, validate_tab_group)),
            "tabGroupLayouts" => {
                is_record_of(value, |entry| validate_tab_group_layout_node(entry, 0))
            }
            "activeWorktreeIdsOnShutdown" | "activeConnectionIdsAtShutdown" => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
            "activeFileIdByWorktree" | "activeBrowserTabIdByWorktree" | "activeTabIdByWorktree" => {
                is_record_of(value, is_nullable_string)
            }
            "markdownFrontmatterVisible" => is_record_of(value, Value::is_boolean),
            "activeTabTypeByWorktree" => is_record_of(value, |entry| {
                matches!(
                    entry.as_str(),
                    Some("terminal" | "editor" | "browser" | "simulator")
                )
            }),
            "browserUrlHistory" => is_array_of(value, validate_browser_history_entry),
            "activeGroupIdByWorktree" => is_record_of(value, Value::is_string),
            "remoteSessionIdsByTabId" => is_record_of_with_key(value, |tab_id, entry| {
                is_terminal_tab_id_str(tab_id) && entry.is_string()
            }),
            "terminalPtyIncarnationsByPaneKey" => is_record_of(value, |entry| {
                entry
                    .as_str()
                    .is_some_and(|text| !text.is_empty() && text.len() <= 128)
            }),
            "lastVisitedAtByWorktreeId" => is_record_of(value, is_nonnegative_number),
            "defaultTerminalTabsAppliedByWorktreeId" => {
                is_record_of(value, |entry| entry == &Value::Bool(true))
            }
            "sleepingAgentSessionsByPaneKey" => is_record_of_with_key(value, |pane_key, entry| {
                validate_sleeping_agent_session(pane_key, entry)
            }),
            "terminalSurfaceTombstonesByPaneKey" => {
                is_record_of(value, validate_terminal_surface_tombstone)
            }
            "terminalTopologyRevisionByRepoId" => is_record_of(value, |entry| {
                entry.as_u64().is_some() || entry.as_i64().is_some_and(|number| number >= 0)
            }),
            _ => return Err(format!("workspace session contains unknown field {name}")),
        };
        if !valid {
            return Err(format!(
                "workspace session field {name} has an invalid shape"
            ));
        }
    }
    Ok(())
}

fn is_nullable_string(value: &Value) -> bool {
    value.is_null() || value.is_string()
}

fn is_record_of(value: &Value, predicate: impl Fn(&Value) -> bool) -> bool {
    value
        .as_object()
        .is_some_and(|record| record.values().all(predicate))
}

fn is_nonnegative_number(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && number >= 0.0)
}

fn is_number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}

fn is_positive_number(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && number > 0.0)
}

fn is_array_of(value: &Value, predicate: impl Fn(&Value) -> bool) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(predicate))
}

fn is_record_of_with_key(value: &Value, predicate: impl Fn(&str, &Value) -> bool) -> bool {
    value
        .as_object()
        .is_some_and(|record| record.iter().all(|(key, entry)| predicate(key, entry)))
}

fn has_field(fields: &Map<String, Value>, name: &str, predicate: impl Fn(&Value) -> bool) -> bool {
    fields.get(name).is_some_and(predicate)
}

fn optional_field(
    fields: &Map<String, Value>,
    name: &str,
    predicate: impl Fn(&Value) -> bool,
) -> bool {
    fields.get(name).is_none_or(predicate)
}

fn is_nullable(value: &Value, predicate: impl Fn(&Value) -> bool) -> bool {
    value.is_null() || predicate(value)
}

fn is_nonempty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

fn is_terminal_tab_id(value: &Value) -> bool {
    value.as_str().is_some_and(is_terminal_tab_id_str)
}

fn is_terminal_tab_id_str(text: &str) -> bool {
    !text.is_empty() && !text.contains(':')
}

fn is_workspace_key(value: &str) -> bool {
    value
        .strip_prefix("worktree:")
        .or_else(|| value.strip_prefix("folder:"))
        .is_some_and(|suffix| !suffix.is_empty())
}

fn validate_terminal_layout_node(value: &Value, depth: usize) -> bool {
    if depth > 128 {
        return false;
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    match fields.get("type").and_then(Value::as_str) {
        Some("leaf") => has_field(fields, "leafId", Value::is_string),
        Some("split") => {
            has_field(fields, "direction", |direction| {
                matches!(direction.as_str(), Some("vertical" | "horizontal"))
            }) && has_field(fields, "first", |node| {
                validate_terminal_layout_node(node, depth + 1)
            }) && has_field(fields, "second", |node| {
                validate_terminal_layout_node(node, depth + 1)
            }) && optional_field(fields, "ratio", is_number)
        }
        _ => false,
    }
}

fn validate_terminal_layout_snapshot(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "root", |root| {
        root.is_null() || validate_terminal_layout_node(root, 0)
    }) && has_field(fields, "activeLeafId", is_nullable_string)
        && has_field(fields, "expandedLeafId", is_nullable_string)
        && [
            "ptyIdsByLeafId",
            "buffersByLeafId",
            "scrollbackRefsByLeafId",
            "titlesByLeafId",
        ]
        .iter()
        .all(|name| optional_field(fields, name, |entry| is_record_of(entry, Value::is_string)))
}

fn validate_ai_vault_title(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "agent", |agent| {
        matches!(agent.as_str(), Some("claude" | "codex"))
    }) && has_field(fields, "sessionId", Value::is_string)
        && has_field(fields, "title", Value::is_string)
}

fn is_tui_agent(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some(
            "claude"
                | "claude-agent-teams"
                | "openclaude"
                | "codex"
                | "autohand"
                | "opencode"
                | "mimo-code"
                | "pi"
                | "omp"
                | "gemini"
                | "antigravity"
                | "aider"
                | "goose"
                | "amp"
                | "kilo"
                | "kiro"
                | "crush"
                | "aug"
                | "cline"
                | "codebuff"
                | "command-code"
                | "continue"
                | "cursor"
                | "droid"
                | "kimi"
                | "mistral-vibe"
                | "qwen-code"
                | "rovo"
                | "hermes"
                | "openclaw"
                | "copilot"
                | "grok"
                | "devin"
                | "ante"
                | "trae"
                | "prime-agent"
        )
    )
}

fn validate_terminal_tab(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "id", is_terminal_tab_id)
        && has_field(fields, "ptyId", is_nullable_string)
        && has_field(fields, "worktreeId", Value::is_string)
        && has_field(fields, "title", Value::is_string)
        && has_field(fields, "customTitle", is_nullable_string)
        && has_field(fields, "color", is_nullable_string)
        && has_field(fields, "sortOrder", is_number)
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "defaultTitle", Value::is_string)
        && optional_field(fields, "generatedTitle", is_nullable_string)
        && optional_field(fields, "aiVaultTitle", |title| {
            is_nullable(title, validate_ai_vault_title)
        })
        && optional_field(fields, "quickCommandLabel", is_nullable_string)
        && optional_field(fields, "isPinned", Value::is_boolean)
        && optional_field(fields, "generation", is_number)
        && optional_field(fields, "startupCwd", is_nonempty_string)
        && optional_field(fields, "launchAgent", is_tui_agent)
}

fn validate_unified_tab(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["id", "entityId", "groupId", "worktreeId", "label"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "contentType", |content_type| {
            matches!(
                content_type.as_str(),
                Some(
                    "terminal"
                        | "editor"
                        | "diff"
                        | "conflict-review"
                        | "check-details"
                        | "browser"
                        | "simulator"
                )
            )
        })
        && has_field(fields, "customLabel", is_nullable_string)
        && has_field(fields, "color", is_nullable_string)
        && has_field(fields, "sortOrder", is_number)
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "generatedLabel", is_nullable_string)
        && optional_field(fields, "aiVaultTitle", |title| {
            is_nullable(title, validate_ai_vault_title)
        })
        && optional_field(fields, "quickCommandLabel", is_nullable_string)
        && optional_field(fields, "isPreview", Value::is_boolean)
        && optional_field(fields, "isPinned", Value::is_boolean)
        && optional_field(fields, "viewMode", |mode| {
            matches!(mode.as_str(), Some("terminal" | "chat"))
        })
}

fn validate_tab_group(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "id", Value::is_string)
        && has_field(fields, "worktreeId", Value::is_string)
        && has_field(fields, "activeTabId", is_nullable_string)
        && has_field(fields, "tabOrder", |order| {
            is_array_of(order, Value::is_string)
        })
        && optional_field(fields, "recentTabIds", |order| {
            is_array_of(order, Value::is_string)
        })
}

fn validate_tab_group_layout_node(value: &Value, depth: usize) -> bool {
    if depth > 128 {
        return false;
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    match fields.get("type").and_then(Value::as_str) {
        Some("leaf") => has_field(fields, "groupId", Value::is_string),
        Some("split") => {
            has_field(fields, "direction", |direction| {
                matches!(direction.as_str(), Some("vertical" | "horizontal"))
            }) && has_field(fields, "first", |node| {
                validate_tab_group_layout_node(node, depth + 1)
            }) && has_field(fields, "second", |node| {
                validate_tab_group_layout_node(node, depth + 1)
            }) && optional_field(fields, "ratio", is_number)
        }
        _ => false,
    }
}

fn validate_persisted_open_file(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["filePath", "relativePath", "worktreeId", "language"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && optional_field(fields, "isPreview", Value::is_boolean)
        && optional_field(fields, "runtimeEnvironmentId", is_nullable_string)
        && optional_field(fields, "externalSshTargetId", |target| {
            target.as_str().is_some_and(|text| !text.trim().is_empty())
        })
        && optional_field(fields, "dirtyDraftContent", Value::is_string)
        && optional_field(fields, "lastKnownDiskSignature", Value::is_string)
        && optional_field(fields, "readOnly", Value::is_boolean)
        && optional_field(fields, "liveTail", Value::is_boolean)
}

fn validate_browser_load_error(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "code", is_number)
        && has_field(fields, "description", Value::is_string)
        && has_field(fields, "validatedUrl", Value::is_string)
}

fn validate_browser_workspace(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["id", "worktreeId", "url", "title"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "loading", Value::is_boolean)
        && has_field(fields, "faviconUrl", is_nullable_string)
        && has_field(fields, "canGoBack", Value::is_boolean)
        && has_field(fields, "canGoForward", Value::is_boolean)
        && has_field(fields, "loadError", |error| {
            is_nullable(error, validate_browser_load_error)
        })
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "label", Value::is_string)
        && optional_field(fields, "sessionProfileId", is_nullable_string)
        && optional_field(fields, "sessionPartition", is_nullable_string)
        && optional_field(fields, "activePageId", is_nullable_string)
        && optional_field(fields, "pageIds", |ids| is_array_of(ids, Value::is_string))
}

fn validate_browser_page(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["id", "workspaceId", "worktreeId", "url", "title"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "loading", Value::is_boolean)
        && has_field(fields, "faviconUrl", is_nullable_string)
        && has_field(fields, "canGoBack", Value::is_boolean)
        && has_field(fields, "canGoForward", Value::is_boolean)
        && has_field(fields, "loadError", |error| {
            is_nullable(error, validate_browser_load_error)
        })
        && has_field(fields, "createdAt", is_number)
        && optional_field(fields, "browserRuntimeEnvironmentId", is_nullable_string)
        && optional_field(fields, "viewportPresetId", |preset| {
            preset.is_null()
                || matches!(
                    preset.as_str(),
                    Some(
                        "mobile-s"
                            | "mobile-m"
                            | "mobile-l"
                            | "tablet"
                            | "laptop"
                            | "laptop-l"
                            | "desktop"
                    )
                )
        })
}

fn validate_browser_history_entry(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    ["url", "normalizedUrl", "title"]
        .iter()
        .all(|name| has_field(fields, name, Value::is_string))
        && has_field(fields, "lastVisitedAt", is_number)
        && has_field(fields, "visitCount", is_number)
}

fn contains_unsafe_text_chars(value: &str) -> bool {
    value
        .chars()
        .any(|character| character <= '\u{1f}' || character == '\u{7f}')
}

fn validate_provider_session(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "key", |key| {
        matches!(key.as_str(), Some("session_id" | "conversation_id"))
    }) && has_field(fields, "id", |id| {
        id.as_str().is_some_and(|text| {
            text == text.trim()
                && !text.is_empty()
                && text.len() <= 512
                && !text.starts_with('-')
                && !contains_unsafe_text_chars(text)
        })
    }) && optional_field(fields, "transcriptPath", |path| {
        path.as_str()
            .is_some_and(|text| !text.is_empty() && !contains_unsafe_text_chars(text))
    })
}

fn validate_launch_environment(value: &Value) -> bool {
    is_record_of_with_key(value, |key, entry| {
        let trimmed = key.trim();
        !trimmed.is_empty()
            && !matches!(trimmed, "__proto__" | "constructor" | "prototype")
            && !trimmed.contains('=')
            && !contains_unsafe_text_chars(trimmed)
            && entry.as_str().is_some_and(|text| !text.contains('\0'))
    })
}

fn validate_sleeping_agent_launch_config(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "agentArgs", Value::is_string)
        && has_field(fields, "agentEnv", validate_launch_environment)
        && optional_field(fields, "agentCommand", Value::is_string)
        && optional_field(fields, "ompResumeFilePath", |path| {
            path.as_str().is_some_and(|text| {
                text == text.trim()
                    && !text.is_empty()
                    && text.len() <= 32 * 1024
                    && !contains_unsafe_text_chars(text)
            })
        })
}

fn is_resumable_agent(value: &str) -> bool {
    matches!(
        value,
        "claude"
            | "codex"
            | "gemini"
            | "antigravity"
            | "opencode"
            | "pi"
            | "mimo-code"
            | "droid"
            | "grok"
            | "devin"
            | "omp"
            | "prime-agent"
    )
}

fn provider_session_matches_agent(agent: &str, provider: &Map<String, Value>) -> bool {
    let key = provider.get("key").and_then(Value::as_str);
    match agent {
        "antigravity" => key == Some("conversation_id"),
        "pi" | "prime-agent" => {
            key == Some("session_id")
                && provider
                    .get("transcriptPath")
                    .is_some_and(is_nonempty_string)
        }
        _ => key == Some("session_id"),
    }
}

fn validate_sleeping_agent_session(pane_key: &str, value: &Value) -> bool {
    if pane_key.is_empty() || matches!(pane_key, "__proto__" | "constructor" | "prototype") {
        return false;
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    let Some(agent) = fields.get("agent").and_then(Value::as_str) else {
        return false;
    };
    let Some(provider) = fields.get("providerSession").and_then(Value::as_object) else {
        return false;
    };
    has_field(fields, "paneKey", |stored| {
        stored.as_str().is_some_and(|text| text == pane_key)
    }) && has_field(fields, "worktreeId", is_nonempty_string)
        && is_resumable_agent(agent)
        && validate_provider_session(&Value::Object(provider.clone()))
        && provider_session_matches_agent(agent, provider)
        && has_field(fields, "prompt", Value::is_string)
        && has_field(fields, "state", |state| {
            matches!(
                state.as_str(),
                Some("working" | "blocked" | "waiting" | "done")
            )
        })
        && has_field(fields, "capturedAt", is_positive_number)
        && has_field(fields, "updatedAt", is_positive_number)
        && optional_field(fields, "tabId", is_terminal_tab_id)
        && optional_field(fields, "terminalTitle", Value::is_string)
        && optional_field(fields, "lastAssistantMessage", Value::is_string)
        && optional_field(fields, "interrupted", Value::is_boolean)
        && optional_field(fields, "connectionId", is_nullable_string)
        && optional_field(
            fields,
            "launchConfig",
            validate_sleeping_agent_launch_config,
        )
        && optional_field(fields, "origin", |origin| {
            matches!(origin.as_str(), Some("worktree-sleep" | "quit" | "live"))
        })
        && optional_field(fields, "automaticResumeBlockedBy", |reason| {
            reason.as_str() == Some("legacy-orchestration-worker")
        })
        && optional_field(fields, "restoreOnTabOpenOnly", Value::is_boolean)
}

fn validate_terminal_surface_tombstone(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    has_field(fields, "worktreeId", Value::is_string)
        && has_field(fields, "parentTabId", is_terminal_tab_id)
        && has_field(fields, "leafId", Value::is_string)
        && has_field(fields, "ptyId", Value::is_string)
        && has_field(fields, "incarnationId", |incarnation| {
            incarnation
                .as_str()
                .is_some_and(|text| !text.is_empty() && text.len() <= 128)
        })
        && has_field(fields, "retiredAt", is_nonnegative_number)
}
