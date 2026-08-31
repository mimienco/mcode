use atomic_write_file::AtomicWriteFile;
use mcode_platform::{DesktopWindowPreferences, WindowBounds};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub mod crossgrade_discovery;
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

    pub fn local_filesystem_roots(&self) -> Result<Vec<PathBuf>, SettingsServiceError> {
        self.local_filesystem_roots_with_wsl_home(mcode_platform::wsl_home_directory)
    }

    pub fn local_filesystem_roots_with_wsl_home(
        &self,
        mut resolve_wsl_home: impl FnMut(&str) -> Option<PathBuf>,
    ) -> Result<Vec<PathBuf>, SettingsServiceError> {
        let mut resolved_homes = BTreeMap::<String, Option<PathBuf>>::new();
        loop {
            let persisted = {
                let _read_guard = self
                    .write_lock
                    .lock()
                    .map_err(|_| SettingsServiceError::LockPoisoned)?;
                self.read_persisted()?
            };
            let roots = {
                let mut resolve_once = |distro: &str| {
                    let key = distro.to_ascii_lowercase();
                    resolved_homes
                        .entry(key)
                        .or_insert_with(|| resolve_wsl_home(distro))
                        .clone()
                };
                local_filesystem_roots_from_persisted(&persisted, &mut resolve_once)
            };
            let still_current = {
                let _read_guard = self
                    .write_lock
                    .lock()
                    .map_err(|_| SettingsServiceError::LockPoisoned)?;
                self.read_persisted()? == persisted
            };
            if still_current {
                return Ok(roots);
            }
        }
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

fn local_filesystem_roots_from_persisted(
    persisted: &Value,
    resolve_wsl_home: &mut impl FnMut(&str) -> Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    let repos = object_array(persisted, "repos");
    let groups = object_array(persisted, "projectGroups");
    let local_repos = repos
        .iter()
        .copied()
        .filter(|repo| record_is_local(repo))
        .collect::<Vec<_>>();

    for repo in &local_repos {
        insert_nonempty_path(&mut roots, repo.get("path"));
        insert_worktree_base_root(
            &mut roots,
            repo,
            persisted.get("settings"),
            resolve_wsl_home,
        );
    }
    if local_repos.is_empty() {
        let workspace_dir = persisted
            .get("settings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("workspaceDir"))
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty());
        if let Some(workspace_dir) = workspace_dir {
            let workspace_dir = PathBuf::from(workspace_dir);
            let resolved = if workspace_dir.is_absolute() {
                workspace_dir
            } else if let Ok(current_dir) = std::env::current_dir() {
                current_dir.join(workspace_dir)
            } else {
                PathBuf::new()
            };
            if !resolved.as_os_str().is_empty() {
                roots.insert(lexically_normalize(&resolved));
            }
        }
    }

    for group in &groups {
        let Some(group_id) = group.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(parent_path) = group.get("parentPath").and_then(Value::as_str) else {
            continue;
        };
        if !is_remote_only_folder_scope(parent_path, group_id, group, &groups, &repos) {
            roots.insert(PathBuf::from(parent_path));
        }
    }

    for workspace in object_array(persisted, "folderWorkspaces") {
        let Some(folder_path) = workspace.get("folderPath").and_then(Value::as_str) else {
            continue;
        };
        let Some(group_id) = workspace.get("projectGroupId").and_then(Value::as_str) else {
            continue;
        };
        let inherited_group = groups
            .iter()
            .copied()
            .find(|group| group.get("id").and_then(Value::as_str) == Some(group_id));
        let explicitly_remote =
            record_is_remote(workspace) || inherited_group.is_some_and(record_is_remote);
        if !explicitly_remote
            && !candidate_repos(folder_path, group_id, &groups, &repos)
                .is_some_and(|candidates| candidates.iter().all(|repo| record_is_remote(repo)))
        {
            roots.insert(PathBuf::from(folder_path));
        }
    }

    roots.into_iter().collect()
}

fn object_array<'a>(persisted: &'a Value, key: &str) -> Vec<&'a Map<String, Value>> {
    persisted
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .collect()
}

fn insert_worktree_base_root(
    roots: &mut BTreeSet<PathBuf>,
    repo: &Map<String, Value>,
    settings: Option<&Value>,
    resolve_wsl_home: &mut impl FnMut(&str) -> Option<PathBuf>,
) {
    let Some(repo_path) = repo.get("path").and_then(Value::as_str) else {
        return;
    };
    let repo_configured = repo
        .get("worktreeBasePath")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty());
    let global_configured = settings
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("workspaceDir"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty());
    let Some(configured) = repo_configured.or(global_configured) else {
        return;
    };
    let configured = resolve_wsl_repo_worktree_base_path(repo_path, configured);
    let configured = if is_wsl_unc_path(repo_path)
        && is_windows_desktop_absolute(&configured)
        && !is_wsl_unc_path(&configured)
    {
        let Some((distro, _)) = parse_wsl_unc_path(repo_path) else {
            return;
        };
        resolve_wsl_home(distro).map_or(configured, |home| {
            format!(
                r"{}\mcode\workspaces",
                home.to_string_lossy().trim_end_matches(['\\', '/'])
            )
        })
    } else {
        configured
    };
    let configured = PathBuf::from(configured);
    let root = if configured.is_absolute() {
        configured
    } else {
        PathBuf::from(repo_path).join(configured)
    };
    roots.insert(lexically_normalize(&root));
}

fn resolve_wsl_repo_worktree_base_path(repo_path: &str, base_path: &str) -> String {
    let Some((distro, _)) = parse_wsl_unc_path(repo_path) else {
        return base_path.to_owned();
    };
    if !base_path.starts_with('/') || base_path.starts_with("//") {
        return base_path.to_owned();
    }
    let collapsed = collapse_posix_dot_segments(base_path);
    format!(
        r"\\wsl.localhost\{distro}{}",
        if collapsed == "/" {
            r"\".to_owned()
        } else {
            collapsed.replace('/', r"\")
        }
    )
}

fn parse_wsl_unc_path(path: &str) -> Option<(&str, String)> {
    let normalized = path.replace('\\', "/");
    let mut components = normalized.strip_prefix("//")?.split('/');
    let host = components.next()?;
    if !host.eq_ignore_ascii_case("wsl.localhost") && !host.eq_ignore_ascii_case("wsl$") {
        return None;
    }
    let distro = components.next()?.trim();
    if distro.is_empty() {
        return None;
    }
    let linux_path = format!("/{}", components.collect::<Vec<_>>().join("/"));
    let distro_start = path.find(distro)?;
    let distro_end = distro_start.checked_add(distro.len())?;
    Some((path.get(distro_start..distro_end)?, linux_path))
}

fn is_wsl_unc_path(path: &str) -> bool {
    parse_wsl_unc_path(path).is_some()
}

fn is_windows_desktop_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\'))
        || (path.starts_with(r"\\") && !is_wsl_unc_path(path))
}

fn collapse_posix_dot_segments(path: &str) -> String {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    format!("/{}", segments.join("/"))
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn is_remote_only_folder_scope(
    folder_path: &str,
    group_id: &str,
    scope: &Map<String, Value>,
    groups: &[&Map<String, Value>],
    repos: &[&Map<String, Value>],
) -> bool {
    record_is_remote(scope)
        || candidate_repos(folder_path, group_id, groups, repos)
            .is_some_and(|candidates| candidates.iter().all(|repo| record_is_remote(repo)))
}

fn candidate_repos<'a>(
    folder_path: &str,
    group_id: &str,
    groups: &[&Map<String, Value>],
    repos: &[&'a Map<String, Value>],
) -> Option<Vec<&'a Map<String, Value>>> {
    let candidates = repos
        .iter()
        .copied()
        .filter(|repo| {
            repo.get("projectGroupId")
                .and_then(Value::as_str)
                .is_some_and(|repo_group_id| group_contains(groups, group_id, repo_group_id))
                || repo
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| path_is_within(folder_path, path))
        })
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}

fn group_contains(groups: &[&Map<String, Value>], ancestor_id: &str, child_id: &str) -> bool {
    let mut current = Some(child_id);
    for _ in 0..=groups.len() {
        let Some(id) = current else {
            return false;
        };
        if id == ancestor_id {
            return true;
        }
        current = groups
            .iter()
            .copied()
            .find(|group| group.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|group| group.get("parentGroupId"))
            .and_then(Value::as_str);
    }
    false
}

fn path_is_within(root: &str, target: &str) -> bool {
    #[cfg(windows)]
    {
        let root_wsl = parse_wsl_unc_path(root);
        let target_wsl = parse_wsl_unc_path(target);
        match (root_wsl, target_wsl) {
            (Some((root_distro, root_tail)), Some((target_distro, target_tail))) => {
                if !root_distro.eq_ignore_ascii_case(target_distro) {
                    return false;
                }
                let root_segments = root_tail
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .collect::<Vec<_>>();
                let target_segments = target_tail
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .collect::<Vec<_>>();
                return target_segments.len() >= root_segments.len()
                    && target_segments
                        .iter()
                        .zip(root_segments)
                        .all(|(target_segment, root_segment)| target_segment == &root_segment);
            }
            (Some(_), None) | (None, Some(_)) => return false,
            (None, None) => {}
        }
        let root = Path::new(root);
        let target = Path::new(target);
        let root = root.components().collect::<Vec<_>>();
        let target = target.components().collect::<Vec<_>>();
        target.len() >= root.len()
            && target.iter().zip(root).all(|(target, root)| {
                target
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&root.as_os_str().to_string_lossy())
            })
    }
    #[cfg(not(windows))]
    {
        Path::new(target).starts_with(Path::new(root))
    }
}

fn record_is_local(record: &Map<String, Value>) -> bool {
    let connection_is_local = record.get("connectionId").is_none_or(Value::is_null);
    let execution_host_is_local = record
        .get("executionHostId")
        .is_none_or(|host| host.is_null() || host.as_str() == Some("local"));
    connection_is_local && execution_host_is_local
}

fn record_is_remote(record: &Map<String, Value>) -> bool {
    !record_is_local(record)
}

fn insert_nonempty_path(roots: &mut BTreeSet<PathBuf>, value: Option<&Value>) {
    if let Some(path) = value
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
    {
        roots.insert(PathBuf::from(path));
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
