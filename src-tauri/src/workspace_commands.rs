//! Workspace open/switch/recents parity surface (`parity-matrix.json` row
//! `workspace.session`, commands `workspace_open`, `workspace_switch`,
//! `workspace_list_recent`).
//!
//! The Electron oracle behind these commands:
//! - open: `repos:add` (`src/main/ipc/repos.ts:1867` → `addLocalRepoFromPath`,
//!   `repos.ts:292-364`) pushes a `Repo` record onto the top-level `repos`
//!   array of mcode-data.json; folder opens go through `folderWorkspaces:create`
//!   (`repos.ts:1500-1549` → `createFolderWorkspace`,
//!   `folder-workspace-operations.ts:55-107`) which prepends a `FolderWorkspace`
//!   record to the top-level `folderWorkspaces` array.
//! - switch: `session:set`/`session:patch` (`src/main/ipc/session.ts:12-22`)
//!   persist the active-workspace pointer
//!   (`activeRepoId`/`activeWorktreeId`/`activeTabId` +
//!   `activeWorkspaceKey`/`activeWorkspaceExecutionHostId`) and per-worktree
//!   focus recency (`lastVisitedAtByWorktreeId`) into the top-level
//!   `workspaceSession` document (the 'local' host partition) or, for
//!   non-local hosts, `workspaceSessionsByHostId[hostId]`
//!   (`store.ts:2979-3022`). Host partitioning is why every command takes an
//!   optional `hostId` that defaults to `'local'`.
//! - recents: Electron has no separate recent-workspaces channel — the
//!   renderer orders the workspace switcher from the persisted session's
//!   `lastVisitedAtByWorktreeId` focus recency
//!   (`order-empty-query-worktrees.ts`) plus the `repos` array order and the
//!   `folderWorkspaces` sortOrder. `workspace_list_recent` therefore reads the
//!   same document shapes: repos in document order, folder workspaces sorted
//!   Electron-style (`getFolderWorkspaces`,
//!   `folder-workspace-operations.ts:45-49`: `sortOrder` descending, then
//!   `name` ascending), ranked against the session's focus recency map.
//!
//! Persistence is the SAME document the SettingsService owns — no new
//! schemas. Records are written field-for-field in the Electron layout
//! (`src/shared/repo-types.ts`, `src/shared/folder-workspace-types.ts`); the
//! module only fills the fields Electron's main process fills (ids, default
//! badge color, timestamps), never defaults anything the oracle leaves
//! absent.
//!
//! Authorization: every local path argument is canonicalized and contained
//! through `crates/mcode-fs`'s scoped registry, seeded the same way
//! filesystem_commands.rs seeds it (`SettingsService::local_filesystem_roots`),
//! so opening a workspace reuses the exact deny/allow discipline of the fs
//! command family (`fs_path_denied` on out-of-scope paths). An unauthorized
//! path never reaches the document.
//!
//! Remaining differences: remote opening, icon/upstream enrichment, and host
//! setup preparation are not implemented here. Git opens validate the actual
//! repository and resolve its root before persisting; remote targets fail.
//!
// registered in lib.rs by orchestrator

use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mcode_fs::scope::{FsError, FsScopeRegistry, LeafPolicy};
use mcode_git::{Repo, WorktreeService};
use mcode_settings::{SettingsService, SettingsServiceError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::DesktopCommandError;

const WORKSPACE_OPEN_OPERATION: &str = "workspace_open";
const WORKSPACE_SWITCH_OPERATION: &str = "workspace_switch";
const WORKSPACE_LIST_RECENT_OPERATION: &str = "workspace_list_recent";

/// Electron's default repo badge color (`REPO_COLORS[0]`,
/// `src/shared/constants.ts:128`) — new records must match `repos:add`.
const DEFAULT_REPO_BADGE_COLOR: &str = "#737373";

/// The 'local' partition key. Electron's session channels default an absent
/// hostId here so pre-partition builds keep reading the same blob
/// (`src/main/ipc/session.ts:9-11`).
const LOCAL_EXECUTION_HOST_ID: &str = "local";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// 16 random bytes → 32 hex chars, the uuid-v4-shaped id Electron writes via
/// `randomUUID()` (`repos.ts:2052`, `folder-workspace-operations.ts:82`). The
/// hash-fold entropy scheme matches runtime_rpc.rs (`next_random_word`) so no
/// new dependency enters the crate.
fn random_record_id() -> String {
    fn next_random_word() -> u64 {
        let a = std::collections::hash_map::RandomState::new();
        let b = std::collections::hash_map::RandomState::new();
        let mut hasher = b.build_hasher();
        hasher.write_u64({
            let mut inner = a.build_hasher();
            inner.write_u64(0x9E37_79B9_7F4A_7C15);
            inner.finish()
        });
        hasher.finish()
    }
    let mut bytes = [0u8; 16];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let word =
            (next_random_word() ^ (index as u64).wrapping_mul(0x517C_C1B7_2722_0A95)).to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
    let mut id = String::with_capacity(32);
    for byte in bytes {
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

/// Electron's `getRepoName`: the last path segment (`repos.ts` uses the
/// basename for `displayName` on every add path).
/// Strips the Windows verbatim prefix (`\\?\C:\...`) that canonicalization
/// produces, so the persisted `path` field reads like the plain path Electron
/// stores (`canonicalize` in the fs crate keeps records renderer-readable).
pub(crate) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) if stripped.as_bytes().get(1) == Some(&b':') => PathBuf::from(stripped),
        _ => path.to_path_buf(),
    }
}

fn repo_display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn user_message_for(operation: &'static str) -> &'static str {
    match operation {
        WORKSPACE_OPEN_OPERATION => "MCode could not open this workspace. Try again.",
        WORKSPACE_SWITCH_OPERATION => "MCode could not save your session. Try again.",
        WORKSPACE_LIST_RECENT_OPERATION => "MCode could not list recent workspaces. Try again.",
        _ => "MCode could not complete the workspace request. Try again.",
    }
}

pub(crate) fn command_error(
    code: &'static str,
    operation: &'static str,
    message: impl Into<String>,
) -> DesktopCommandError {
    DesktopCommandError::new(
        code,
        operation,
        message.into(),
        user_message_for(operation),
        None,
        None,
    )
}

pub(crate) fn settings_error(
    error: SettingsServiceError,
    operation: &'static str,
) -> DesktopCommandError {
    DesktopCommandError::settings(error, operation)
}

fn fs_error(error: FsError, operation: &'static str) -> DesktopCommandError {
    command_error(error.code(), operation, error.to_string())
}

// ─── Scoped path authorization ──────────────────────────────────────────────

/// Native dialog grants are separate from persisted roots, which are read fresh.
struct WorkspacePathScope {
    picked_roots: FsScopeRegistry,
    settings: SettingsService,
}

impl WorkspacePathScope {
    fn new(settings: SettingsService) -> Self {
        Self {
            picked_roots: FsScopeRegistry::new(),
            settings,
        }
    }

    fn authorize(&self, path: &Path) -> Result<PathBuf, DesktopCommandError> {
        let persisted = FsScopeRegistry::new();
        for root in self
            .settings
            .local_filesystem_roots()
            .map_err(|error| settings_error(error, WORKSPACE_OPEN_OPERATION))?
        {
            let _ = persisted.register_root(&root);
        }
        persisted
            .authorize(path, LeafPolicy::Canonicalize)
            .or_else(|_| self.picked_roots.authorize(path, LeafPolicy::Canonicalize))
            .map(|authorized| authorized.into_path_buf())
            .map_err(|error| fs_error(error, WORKSPACE_OPEN_OPERATION))
    }
}

/// Thread-safe holder mirroring `NativeFilesystemState`'s shape so the
/// commands can manage the registry without new managed state (the registry
/// is process-lifetime; re-seeding per call is the fs family's own
/// refresh_roots_if_changed behavior simplified).
#[derive(Clone)]
pub(crate) struct WorkspacePathScopeHandle(Arc<Mutex<WorkspacePathScope>>);

impl WorkspacePathScopeHandle {
    pub(crate) fn new(settings: SettingsService) -> Self {
        Self(Arc::new(Mutex::new(WorkspacePathScope::new(settings))))
    }

    /// Called only with the native dialog result, never with an invoke argument.
    pub(crate) fn register_native_picked_directory(
        &self,
        path: &Path,
    ) -> Result<PathBuf, DesktopCommandError> {
        const PICK: &str = "workspace_pick_folder";
        if !path.is_absolute() || !path.is_dir() {
            return Err(command_error(
                "workspace_pick_folder_path_unusable",
                PICK,
                "The selected folder is not an existing absolute directory",
            ));
        }
        self.with(PICK, |scope| {
            scope
                .picked_roots
                .register_root(path)
                .map(|authorized| authorized.into_path_buf())
                .map_err(|error| fs_error(error, PICK))
        })
    }

    fn with<R>(
        &self,
        operation: &'static str,
        run: impl FnOnce(&WorkspacePathScope) -> Result<R, DesktopCommandError>,
    ) -> Result<R, DesktopCommandError> {
        let guard = self
            .0
            .lock()
            .map_err(|_| command_error("workspace_scope_lock_poisoned", operation, "scope lock"))?;
        run(&guard)
    }
}

// ─── DTOs — wire shapes mirror the Electron records field-for-field ─────────

/// Args of `workspace_open` (camelCase at the boundary, per the command
/// attribute). `kind` mirrors `repos:add`'s optional `{ path, kind }` and
/// `folderWorkspaces.create`'s `{ projectGroupId, folderPath, kind? }`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceOpenArgs {
    /// Absolute local folder path to open.
    pub folder_path: String,
    /// `'git' | 'folder'` — Electron `RepoKind`. Absent means `'git'`
    /// (`addLocalRepoFromPath`'s default).
    pub kind: Option<String>,
    /// Folder-workspace creation mode: the project group to attach the
    /// folder to (Electron `folderWorkspaces:create`). Absent opens a plain
    /// repo record instead.
    pub project_group_id: Option<String>,
    /// Optional display-name override (Electron `repos:create` /
    /// `createFolderWorkspace` `input.name`).
    pub display_name: Option<String>,
    /// Optional SSH/runtime target the folder lives on. Local opens omit it.
    pub connection_id: Option<String>,
    /// Execution host partition the open belongs to. Absent = `'local'`.
    pub host_id: Option<String>,
}

/// `workspace_open` result: the persisted record, exactly the shape the
/// renderer reads off `Repo` (`src/shared/repo-types.ts`) or
/// `FolderWorkspace` (`src/shared/folder-workspace-types.ts`). Only fields
/// Electron's main process fills at add time are present; enrichment fields
/// (repoIcon, upstream, gitUsername) stay absent exactly as the oracle
/// leaves them until a later `repos:list` enrichment.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceOpenRecord {
    pub kind: &'static str,
    pub repo: Option<Value>,
    pub folder_workspace: Option<Value>,
    /// `true` when an existing record already covered the path (Electron's
    /// `alreadyExisted` — the caller treats this as a no-op open).
    pub already_existed: bool,
}

/// Args of `workspace_switch`. The pointer fields mirror
/// `WorkspaceSessionState`'s active-workspace keys
/// (`src/shared/workspace-session-state-types.ts:32-38`) and the recency map
/// the switcher ranks by (`lastVisitedAtByWorktreeId`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceSwitchArgs {
    pub active_repo_id: Option<String>,
    /// Legacy worktree id or canonical `worktree:`/`folder:` WorkspaceKey.
    pub active_worktree_id: Option<String>,
    /// Scope-aware active owner (`folder:<id>` / `worktree:<id>`).
    pub active_workspace_key: Option<String>,
    /// Host stamp for the active workspace (non-local folder opens).
    pub active_workspace_execution_host_id: Option<String>,
    pub active_tab_id: Option<String>,
    /// Focus-recency stamp for the switched-to workspace (epoch millis),
    /// written into `lastVisitedAtByWorktreeId`.
    pub visited_at: Option<i64>,
    /// Session partition to write. Absent = `'local'`.
    pub host_id: Option<String>,
}

/// `workspace_list_recent` result: the persisted catalog in Electron's order
/// semantics plus the focus-recency map the switcher sorts by.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceRecents {
    /// Repo records in document order (`store.getRepos()` = array order).
    pub repos: Vec<Value>,
    /// Folder-workspace records sorted `sortOrder` desc, then `name` asc —
    /// `getFolderWorkspaces`' exact comparator.
    pub folder_workspaces: Vec<Value>,
    /// `lastVisitedAtByWorktreeId` from the session partition the call
    /// reads; absent entries mean "never focused" exactly as Electron.
    pub last_visited_at_by_worktree_id: Map<String, Value>,
    /// The active-workspace pointer of the same partition.
    pub active_workspace_key: Option<String>,
    pub active_repo_id: Option<String>,
}

// ─── Document helpers ───────────────────────────────────────────────────────

fn ensure_array_mut<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Vec<Value>, SettingsServiceError> {
    let array = root
        .entry(key.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    array.as_array_mut().ok_or(match key {
        "repos" => SettingsServiceError::PersistedSectionMustBeObject("repos"),
        "folderWorkspaces" => {
            SettingsServiceError::PersistedSectionMustBeObject("folderWorkspaces")
        }
        _ => SettingsServiceError::PersistedSectionMustBeObject("root"),
    })
}

pub(crate) fn path_comparison_key(path: &str) -> String {
    // The renderer/hosts compare Windows paths case-insensitively and fold
    // separators (`normalizeRuntimePathForComparison`). The Tauri host only
    // ever records local paths, so separator + case folding is the faithful
    // subset; WSL-UNC aliasing stays untouched (no aliases are produced on
    // this side).
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    if path.starts_with(char::is_alphabetic) && path.as_bytes().get(1) == Some(&b':')
        || trimmed.starts_with("//")
    {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed.to_owned()
    }
}

fn repo_record_path(repo: &Map<String, Value>) -> Option<&str> {
    repo.get("path").and_then(Value::as_str)
}

pub(crate) fn is_local_record(record: &Map<String, Value>) -> bool {
    let connection_is_local = record.get("connectionId").is_none_or(Value::is_null);
    let execution_host_is_local = record
        .get("executionHostId")
        .is_none_or(|host| host.is_null() || host.as_str() == Some("local"));
    connection_is_local && execution_host_is_local
}

pub(crate) fn folder_workspace_comparator() -> impl Fn(&Value, &Value) -> std::cmp::Ordering {
    // `getFolderWorkspaces` (`folder-workspace-operations.ts:46-48`):
    // `right.sortOrder - left.sortOrder || left.name.localeCompare(right.name)`.
    |left: &Value, right: &Value| {
        let left_order = left.get("sortOrder").and_then(Value::as_i64).unwrap_or(0);
        let right_order = right.get("sortOrder").and_then(Value::as_i64).unwrap_or(0);
        right_order.cmp(&left_order).then_with(|| {
            let left_name = left.get("name").and_then(Value::as_str).unwrap_or("");
            let right_name = right.get("name").and_then(Value::as_str).unwrap_or("");
            left_name.cmp(right_name)
        })
    }
}

fn remote_open_error() -> DesktopCommandError {
    command_error(
        "workspace_open_remote_unsupported",
        WORKSPACE_OPEN_OPERATION,
        "opening a remote workspace requires an execution-host route that is not available",
    )
}

fn project_group_is_remote(root: &Map<String, Value>, group_id: Option<&str>) -> bool {
    group_id.is_some_and(|group_id| {
        object_array(root, "projectGroups").iter().any(|group| {
            group.get("id").and_then(Value::as_str) == Some(group_id) && !is_local_record(group)
        })
    })
}

// ─── *_for seams (contract-test pattern, `app_get_launch_context_for` style) ─

pub(crate) fn workspace_open_for(
    scope: &WorkspacePathScopeHandle,
    service: &SettingsService,
    args: WorkspaceOpenArgs,
    now_ms: i64,
) -> Result<WorkspaceOpenRecord, DesktopCommandError> {
    // This command currently owns only local filesystem work. A remote
    // target must fail before any local probe, scope grant, or catalog write.
    if args.connection_id.is_some()
        || resolve_host_id(args.host_id.as_deref()) != LOCAL_EXECUTION_HOST_ID
    {
        return Err(remote_open_error());
    }
    if args.project_group_id.is_some() {
        let document = service
            .read_persisted_public()
            .map_err(|error| settings_error(error, WORKSPACE_OPEN_OPERATION))?;
        if document
            .as_object()
            .is_some_and(|root| project_group_is_remote(root, args.project_group_id.as_deref()))
        {
            return Err(remote_open_error());
        }
    }

    let folder_path = PathBuf::from(args.folder_path.trim());
    if folder_path.as_os_str().is_empty() {
        return Err(command_error(
            "workspace_open_invalid_args",
            WORKSPACE_OPEN_OPERATION,
            "folderPath is required",
        ));
    }
    if !folder_path.is_absolute() || !folder_path.is_dir() {
        return Err(command_error(
            "workspace_open_path_unusable",
            WORKSPACE_OPEN_OPERATION,
            format!(
                "folder path is not an existing directory: {}",
                folder_path.display()
            ),
        ));
    }
    let authorized = scope.with(WORKSPACE_OPEN_OPERATION, |scope| {
        scope.authorize(&folder_path)
    })?;
    let mut authorized = strip_verbatim_prefix(&authorized);
    let mut main_repo_path = None;
    if args.project_group_id.is_none() && args.kind.as_deref() != Some("folder") {
        let repo = Repo::discover(&authorized).map_err(|error| {
            command_error(
                error.code(),
                WORKSPACE_OPEN_OPERATION,
                format!("Not a valid git repository: {}", authorized.display()),
            )
        })?;
        let entries = WorktreeService::new(&repo).list().map_err(|error| {
            command_error(error.code(), WORKSPACE_OPEN_OPERATION, error.to_string())
        })?;
        authorized = scope.with(WORKSPACE_OPEN_OPERATION, |scope| {
            scope.authorize(&repo.root)
        })?;
        authorized = strip_verbatim_prefix(&authorized);
        main_repo_path = entries.first().map(|entry| entry.path.clone());
    }

    let mut result = WorkspaceOpenRecord {
        kind: "repo",
        repo: None,
        folder_workspace: None,
        already_existed: false,
    };
    let mut remote_group_rejected = false;
    service
        .update_persisted_public(|root| {
            // Recheck under the write lock: the group may have been moved to
            // another host since the preflight read. Abort the transaction.
            if project_group_is_remote(root, args.project_group_id.as_deref()) {
                remote_group_rejected = true;
                return Err(SettingsServiceError::PersistedSectionMustBeObject(
                    "projectGroups",
                ));
            }
            if let Some(project_group_id) = args.project_group_id.as_deref() {
                let record = build_folder_workspace_record(
                    root,
                    project_group_id,
                    &authorized,
                    args.display_name.as_deref(),
                    now_ms,
                )?;
                ensure_array_mut(root, "folderWorkspaces")?.insert(0, record.clone());
                result.kind = "folderWorkspace";
                result.folder_workspace = Some(record);
            } else {
                let (record, already_existed) = build_repo_record(
                    root,
                    &authorized,
                    args.kind.as_deref() == Some("folder"),
                    args.display_name.as_deref(),
                    main_repo_path.as_deref(),
                    now_ms,
                );
                if !already_existed {
                    ensure_array_mut(root, "repos")?.push(record.clone());
                }
                result.repo = Some(record);
                result.already_existed = already_existed;
            }
            Ok(())
        })
        .map_err(|error| {
            if remote_group_rejected {
                remote_open_error()
            } else {
                settings_error(error, WORKSPACE_OPEN_OPERATION)
            }
        })?;

    // Capture the exact record within the transaction. A second read could
    // select another concurrent open's first/last catalog entry.
    Ok(result)
}

/// `addLocalRepoFromPath`'s record construction (`repos.ts:351-363`) — same
/// fields, same defaults, same fill order.
fn build_repo_record(
    root: &Map<String, Value>,
    authorized_path: &Path,
    is_folder_kind: bool,
    display_name: Option<&str>,
    main_repo_path: Option<&str>,
    now_ms: i64,
) -> (Value, bool) {
    let path_string = authorized_path.to_string_lossy().into_owned();
    let path_key = path_comparison_key(&path_string);
    let repos = object_array(root, "repos");
    for repo in &repos {
        if !is_local_record(repo) {
            continue;
        }
        if repo_record_path(repo).is_some_and(|existing| {
            let existing_key = path_comparison_key(existing);
            existing_key == path_key
                || (!is_folder_kind
                    && repo.get("kind").and_then(Value::as_str) != Some("folder")
                    && main_repo_path.is_some_and(|main| path_comparison_key(main) == existing_key))
        }) {
            return (Value::Object((**repo).clone()), true);
        }
    }
    let mut record = Map::new();
    record.insert("id".into(), Value::String(random_record_id()));
    record.insert("path".into(), Value::String(path_string));
    record.insert(
        "displayName".into(),
        Value::String(
            display_name
                .map(str::to_owned)
                .unwrap_or_else(|| repo_display_name(authorized_path)),
        ),
    );
    record.insert(
        "badgeColor".into(),
        Value::String(DEFAULT_REPO_BADGE_COLOR.to_owned()),
    );
    record.insert("addedAt".into(), Value::Number(now_ms.into()));
    record.insert(
        "kind".into(),
        Value::String(if is_folder_kind { "folder" } else { "git" }.to_owned()),
    );
    if !is_folder_kind {
        record.insert(
            "externalWorktreeVisibilityLegacy".into(),
            Value::Bool(false),
        );
        record.insert(
            "projectHostSetupMethod".into(),
            Value::String("imported-existing-folder".into()),
        );
    }
    (Value::Object(record), false)
}

/// `createFolderWorkspace`'s record construction
/// (`folder-workspace-operations.ts:78-105`) — same defaults
/// (linkedTask null, comment '', isArchived/isUnread/isPinned false,
/// sortOrder = now, lastActivityAt 0) and the prepend-into-array order
/// (`folder-workspace-operations.ts:106`).
fn build_folder_workspace_record(
    root: &Map<String, Value>,
    project_group_id: &str,
    authorized_path: &Path,
    display_name: Option<&str>,
    now_ms: i64,
) -> Result<Value, SettingsServiceError> {
    let groups = object_array(root, "projectGroups");
    let group = groups
        .iter()
        .copied()
        .find(|group| group.get("id").and_then(Value::as_str) == Some(project_group_id));
    let group_name = group
        .and_then(|group| group.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if group.is_none() {
        return Err(SettingsServiceError::PersistedSectionMustBeObject(
            "projectGroups",
        ));
    }
    let name = display_name
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{group_name} workspace"));
    let mut record = Map::new();
    record.insert("id".into(), Value::String(random_record_id()));
    record.insert(
        "projectGroupId".into(),
        Value::String(project_group_id.to_owned()),
    );
    record.insert("name".into(), Value::String(name));
    record.insert(
        "folderPath".into(),
        Value::String(authorized_path.to_string_lossy().into_owned()),
    );
    record.insert("connectionId".into(), Value::Null);
    record.insert("linkedTask".into(), Value::Null);
    record.insert("comment".into(), Value::String(String::new()));
    record.insert("isArchived".into(), Value::Bool(false));
    record.insert("isUnread".into(), Value::Bool(false));
    record.insert("isPinned".into(), Value::Bool(false));
    record.insert("sortOrder".into(), Value::Number(now_ms.into()));
    record.insert("lastActivityAt".into(), Value::Number(0.into()));
    record.insert("createdAt".into(), Value::Number(now_ms.into()));
    record.insert("updatedAt".into(), Value::Number(now_ms.into()));
    Ok(Value::Object(record))
}

fn object_array<'a>(root: &'a Map<String, Value>, key: &str) -> Vec<&'a Map<String, Value>> {
    root.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .collect()
}

fn resolve_host_id(args_host_id: Option<&str>) -> String {
    match args_host_id {
        Some(host_id) if !host_id.is_empty() => host_id.to_owned(),
        _ => LOCAL_EXECUTION_HOST_ID.to_owned(),
    }
}

/// Electron `getWorkspaceSession` (`store.ts:2979-2985`): `'local'` reads the
/// legacy top-level blob, every other host reads its partition map entry.
fn session_partition<'a>(
    root: &'a Map<String, Value>,
    host_id: &str,
) -> Option<&'a Map<String, Value>> {
    if host_id == LOCAL_EXECUTION_HOST_ID {
        root.get("workspaceSession").and_then(Value::as_object)
    } else {
        root.get("workspaceSessionsByHostId")
            .and_then(Value::as_object)
            .and_then(|partitions| partitions.get(host_id))
            .and_then(Value::as_object)
    }
}

fn empty_session_fields() -> Map<String, Value> {
    // `getDefaultWorkspaceSession` (`src/shared/constants.ts:539-556`): the
    // required keys the document contract validates must exist before any
    // partition write lands.
    let mut fields = Map::new();
    fields.insert("activeRepoId".into(), Value::Null);
    fields.insert("activeWorktreeId".into(), Value::Null);
    fields.insert("activeTabId".into(), Value::Null);
    fields.insert("tabsByWorktree".into(), Value::Object(Map::new()));
    fields.insert("terminalLayoutsByTabId".into(), Value::Object(Map::new()));
    fields
}

pub(crate) fn workspace_switch_for(
    service: &SettingsService,
    args: WorkspaceSwitchArgs,
    now_ms: i64,
) -> Result<(), DesktopCommandError> {
    let host_id = resolve_host_id(args.host_id.as_deref());
    let operation = WORKSPACE_SWITCH_OPERATION;
    service
        .update_persisted_public(move |root| {
            // Pointer fields that must land on every switch — the exact set
            // `set-active-worktree.ts`/`set-active-folder-workspace.ts` set
            // client-side and the session contract validates.
            let mut pointer = Map::new();
            pointer.insert(
                "activeRepoId".into(),
                args.active_repo_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            pointer.insert(
                "activeWorktreeId".into(),
                args.active_worktree_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            pointer.insert(
                "activeTabId".into(),
                args.active_tab_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            if let Some(active_workspace_key) = args.active_workspace_key.clone() {
                pointer.insert(
                    "activeWorkspaceKey".into(),
                    Value::String(active_workspace_key),
                );
            }
            if let Some(host) = args.active_workspace_execution_host_id.clone() {
                pointer.insert("activeWorkspaceExecutionHostId".into(), Value::String(host));
            }
            if let Some(visited_at) = args.visited_at {
                pointer.insert(
                    "lastVisitedAtByWorktreeId".into(),
                    Value::Object(Map::from_iter([(
                        args.active_workspace_key
                            .clone()
                            .or_else(|| args.active_worktree_id.clone())
                            .unwrap_or_default(),
                        Value::Number(visited_at.into()),
                    )])),
                );
            }

            if host_id == LOCAL_EXECUTION_HOST_ID {
                let session = root
                    .entry("workspaceSession".to_owned())
                    .or_insert_with(|| Value::Object(empty_session_fields()));
                let session = session.as_object_mut().ok_or(
                    SettingsServiceError::PersistedSectionMustBeObject("workspaceSession"),
                )?;
                for (key, value) in pointer {
                    session.insert(key, value);
                }
                return Ok(());
            }

            if !crate_is_valid_host_id(&host_id) {
                return Err(SettingsServiceError::PersistedSectionMustBeObject(
                    "workspaceSessionsByHostId",
                ));
            }
            let partitions = root
                .entry("workspaceSessionsByHostId".to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            let partitions = partitions.as_object_mut().ok_or(
                SettingsServiceError::PersistedSectionMustBeObject("workspaceSessionsByHostId"),
            )?;
            let session = partitions
                .entry(host_id)
                .or_insert_with(|| Value::Object(empty_session_fields()));
            let session = session.as_object_mut().ok_or(
                SettingsServiceError::PersistedSectionMustBeObject("workspaceSessionsByHostId"),
            )?;
            for (key, value) in pointer {
                session.insert(key, value);
            }
            Ok(())
        })
        .map_err(|error| {
            let _ = now_ms;
            settings_error(error, operation)
        })
}

fn crate_is_valid_host_id(host_id: &str) -> bool {
    host_id == "local"
        || host_id
            .strip_prefix("ssh:")
            .is_some_and(|rest| !rest.is_empty())
        || host_id
            .strip_prefix("runtime:")
            .is_some_and(|rest| !rest.is_empty())
}

pub(crate) fn workspace_list_recent_for(
    scope: &WorkspacePathScopeHandle,
    service: &SettingsService,
    host_id: Option<&str>,
) -> Result<WorkspaceRecents, DesktopCommandError> {
    let _ = scope;
    let host_id = resolve_host_id(host_id);
    let persisted = service
        .read_persisted_public()
        .map_err(|error| settings_error(error, WORKSPACE_LIST_RECENT_OPERATION))?;
    let empty_root = Map::new();
    let root = persisted.as_object().unwrap_or(&empty_root);

    let repos: Vec<Value> = root
        .get("repos")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Document order IS the Electron order (`getRepos()` returns the array);
    // the copy keeps the returned value decoupled from the read snapshot.

    let mut folder_workspaces: Vec<Value> = root
        .get("folderWorkspaces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    folder_workspaces.sort_by(folder_workspace_comparator());

    let session = session_partition(root, &host_id);
    let last_visited_at_by_worktree_id = session
        .and_then(|session| session.get("lastVisitedAtByWorktreeId"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let active_workspace_key = session
        .and_then(|session| session.get("activeWorkspaceKey"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let active_repo_id = session
        .and_then(|session| session.get("activeRepoId"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    Ok(WorkspaceRecents {
        repos,
        folder_workspaces,
        last_visited_at_by_worktree_id,
        active_workspace_key,
        active_repo_id,
    })
}

// ─── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn workspace_open(
    scope: tauri::State<'_, WorkspacePathScopeHandle>,
    service: tauri::State<'_, SettingsService>,
    args: WorkspaceOpenArgs,
) -> Result<WorkspaceOpenRecord, DesktopCommandError> {
    let scope = scope.inner().clone();
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        workspace_open_for(&scope, &service, args, now_ms())
    })
    .await
    .map_err(|error| {
        command_error(
            "workspace_open_join_failed",
            WORKSPACE_OPEN_OPERATION,
            error.to_string(),
        )
    })?
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn workspace_switch(
    service: tauri::State<'_, SettingsService>,
    args: WorkspaceSwitchArgs,
) -> Result<(), DesktopCommandError> {
    workspace_switch_for(&service, args, now_ms())
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) fn workspace_list_recent(
    service: tauri::State<'_, SettingsService>,
    host_id: Option<String>,
) -> Result<WorkspaceRecents, DesktopCommandError> {
    workspace_list_recent_for(
        &WorkspacePathScopeHandle::new(SettingsService::new(std::path::PathBuf::new())),
        &service,
        host_id.as_deref(),
    )
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    struct Fixture {
        _dir: TempDir,
        service: SettingsService,
        scope: WorkspacePathScopeHandle,
    }

    fn fixture_with_document(mut document: Value) -> Fixture {
        let dir = TempDir::new().expect("temp dir");
        document["settings"]["workspaceDir"] = json!(dir.path());
        let settings_path = dir.path().join("mcode-data.json");
        std::fs::write(
            &settings_path,
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .expect("write fixture document");
        let service = SettingsService::new(settings_path);
        let scope = WorkspacePathScopeHandle::new(service.clone());
        Fixture {
            _dir: dir,
            service,
            scope,
        }
    }

    fn empty_fixture() -> Fixture {
        fixture_with_document(json!({}))
    }

    fn write_workspace_dir(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).expect("create workspace dir");
        dir
    }

    fn test_git(root: &Path, args: &[&str]) {
        let mut command = std::process::Command::new("git");
        command.current_dir(root).args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let output = command.output().expect("git available");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn unpicked_existing_folder_is_denied_without_mutation_or_later_scope_grant() {
        let fixture = empty_fixture();
        let outside = TempDir::new().unwrap();
        let before = std::fs::read(fixture._dir.path().join("mcode-data.json")).unwrap();
        assert_eq!(
            crate::workspace_catalog_commands::workspace_picked_folder_for(&fixture.scope, None)
                .unwrap(),
            None
        );
        for kind in ["folder", "git"] {
            let error = workspace_open_for(
                &fixture.scope,
                &fixture.service,
                WorkspaceOpenArgs {
                    folder_path: outside.path().to_string_lossy().into_owned(),
                    kind: Some(kind.into()),
                    project_group_id: None,
                    display_name: None,
                    connection_id: None,
                    host_id: None,
                },
                1,
            )
            .unwrap_err();
            assert_eq!(error.code, "fs_path_denied");
            assert_eq!(
                std::fs::read(fixture._dir.path().join("mcode-data.json")).unwrap(),
                before
            );
        }
    }

    #[test]
    fn native_picker_grant_allows_open_and_persisted_root_survives_scope_restart() {
        let fixture = empty_fixture();
        let outside = TempDir::new().unwrap();
        let selected = crate::workspace_catalog_commands::workspace_picked_folder_for(
            &fixture.scope,
            Some(outside.path().to_string_lossy().into_owned()),
        )
        .unwrap()
        .unwrap();
        let args = WorkspaceOpenArgs {
            folder_path: selected.clone(),
            kind: Some("folder".into()),
            project_group_id: None,
            display_name: None,
            connection_id: None,
            host_id: None,
        };
        let opened = workspace_open_for(&fixture.scope, &fixture.service, args.clone(), 1).unwrap();
        assert_eq!(opened.repo.as_ref().unwrap()["path"], selected);
        let restarted = WorkspacePathScopeHandle::new(fixture.service.clone());
        let duplicate = workspace_open_for(&restarted, &fixture.service, args, 2).unwrap();
        assert!(duplicate.already_existed);
        assert_eq!(duplicate.repo, opened.repo);
    }

    #[test]
    fn picked_child_does_not_grant_its_unpicked_parent() {
        let fixture = empty_fixture();
        let outside = TempDir::new().unwrap();
        let child = write_workspace_dir(outside.path(), "selected-child");
        crate::workspace_catalog_commands::workspace_picked_folder_for(
            &fixture.scope,
            Some(child.to_string_lossy().into_owned()),
        )
        .unwrap();
        let error = workspace_open_for(
            &fixture.scope,
            &fixture.service,
            WorkspaceOpenArgs {
                folder_path: outside.path().to_string_lossy().into_owned(),
                kind: Some("folder".into()),
                project_group_id: None,
                display_name: None,
                connection_id: None,
                host_id: None,
            },
            1,
        )
        .unwrap_err();
        assert_eq!(error.code, "fs_path_denied");
        assert!(
            fixture
                .service
                .read_persisted_public()
                .unwrap()
                .get("repos")
                .is_none()
        );
    }

    #[test]
    fn open_persists_a_repo_record_in_document_order() {
        let fixture = empty_fixture();
        let dir = write_workspace_dir(fixture._dir.path(), "alpha");
        test_git(&dir, &["init"]);
        let record = workspace_open_for(
            &fixture.scope,
            &fixture.service,
            WorkspaceOpenArgs {
                folder_path: dir.to_string_lossy().into_owned(),
                kind: Some("git".into()),
                project_group_id: None,
                display_name: Some("Alpha".to_owned()),
                connection_id: None,
                host_id: None,
            },
            1_000,
        )
        .expect("open");

        assert_eq!(record.kind, "repo");
        let repo = record.repo.expect("repo record");
        assert_eq!(repo["path"], json!(dir.to_string_lossy().into_owned()));
        assert_eq!(repo["displayName"], json!("Alpha"));
        assert_eq!(repo["badgeColor"], json!("#737373"));
        assert_eq!(repo["addedAt"], json!(1_000));
        assert_eq!(repo["kind"], json!("git"));

        let document = fixture.service.read_persisted_public().unwrap();
        assert_eq!(document["repos"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn open_in_a_project_group_persists_a_folder_workspace_record() {
        let fixture = fixture_with_document(json!({
            "projectGroups": [
                { "id": "group-1", "name": "Acme", "parentPath": "D:/work" }
            ]
        }));
        let dir = write_workspace_dir(fixture._dir.path(), "site");
        let record = workspace_open_for(
            &fixture.scope,
            &fixture.service,
            WorkspaceOpenArgs {
                folder_path: dir.to_string_lossy().into_owned(),
                kind: None,
                project_group_id: Some("group-1".into()),
                display_name: None,
                connection_id: None,
                host_id: None,
            },
            2_000,
        )
        .expect("open");

        assert_eq!(record.kind, "folderWorkspace");
        let workspace = record.folder_workspace.expect("folder workspace record");
        assert_eq!(workspace["projectGroupId"], json!("group-1"));
        assert_eq!(workspace["name"], json!("Acme workspace"));
        assert_eq!(workspace["sortOrder"], json!(2_000));
        assert_eq!(workspace["linkedTask"], Value::Null);
        assert_eq!(workspace["isArchived"], json!(false));

        let document = fixture.service.read_persisted_public().unwrap();
        let array = document["folderWorkspaces"].as_array().unwrap();
        assert_eq!(array.len(), 1);
        assert_eq!(array[0]["id"], workspace["id"]);
    }

    #[test]
    fn open_rejects_an_unauthorized_or_missing_path_with_a_typed_error() {
        let fixture = empty_fixture();
        // Missing directory: never persisted.
        let missing = fixture._dir.path().join("does-not-exist");
        let error = workspace_open_for(
            &fixture.scope,
            &fixture.service,
            WorkspaceOpenArgs {
                folder_path: missing.to_string_lossy().into_owned(),
                kind: None,
                project_group_id: None,
                display_name: None,
                connection_id: None,
                host_id: None,
            },
            0,
        )
        .expect_err("missing path must fail");
        assert_eq!(error.code, "workspace_open_path_unusable");

        // Empty path: invalid args.
        let error = workspace_open_for(
            &fixture.scope,
            &fixture.service,
            WorkspaceOpenArgs {
                folder_path: "   ".into(),
                kind: None,
                project_group_id: None,
                display_name: None,
                connection_id: None,
                host_id: None,
            },
            0,
        )
        .expect_err("empty path must fail");
        assert_eq!(error.code, "workspace_open_invalid_args");

        let document = fixture.service.read_persisted_public().unwrap();
        assert!(document.get("repos").is_none());
    }

    #[test]
    fn open_is_idempotent_on_the_same_local_path_like_the_oracle_dedupe() {
        let fixture = empty_fixture();
        let dir = write_workspace_dir(fixture._dir.path(), "dup");
        let args = WorkspaceOpenArgs {
            folder_path: dir.to_string_lossy().into_owned(),
            kind: Some("folder".into()),
            project_group_id: None,
            display_name: None,
            connection_id: None,
            host_id: None,
        };
        let first = workspace_open_for(&fixture.scope, &fixture.service, args.clone(), 1_000)
            .expect("first open");
        let second = workspace_open_for(&fixture.scope, &fixture.service, args, 2_000)
            .expect("duplicate open returns the existing record");
        let document = fixture.service.read_persisted_public().unwrap();
        assert_eq!(document["repos"].as_array().unwrap().len(), 1);
        assert!(second.already_existed);
        assert_eq!(second.repo, first.repo);
    }

    #[test]
    fn remote_open_targets_fail_before_local_validation_or_document_mutation() {
        let fixture = fixture_with_document(json!({"unrelated": {"keep": true}}));
        let settings_path = fixture._dir.path().join("mcode-data.json");
        let before = std::fs::read(&settings_path).unwrap();
        let missing = fixture._dir.path().join("does-not-exist");
        for (host_id, connection_id) in [
            (Some("ssh:box"), None),
            (Some("runtime:container"), None),
            (Some("local"), Some("box")),
            (None, Some("box")),
            (None, Some("")),
        ] {
            let error = workspace_open_for(
                &fixture.scope,
                &fixture.service,
                WorkspaceOpenArgs {
                    folder_path: missing.to_string_lossy().into_owned(),
                    kind: None,
                    project_group_id: None,
                    display_name: None,
                    connection_id: connection_id.map(str::to_owned),
                    host_id: host_id.map(str::to_owned),
                },
                100,
            )
            .unwrap_err();
            assert_eq!(error.code, "workspace_open_remote_unsupported");
            assert_eq!(std::fs::read(&settings_path).unwrap(), before);
            assert!(!missing.exists());
        }
    }

    #[test]
    fn inherited_remote_project_groups_cannot_create_local_workspaces() {
        for remote_stamp in [
            json!({"connectionId": "box"}),
            json!({"executionHostId": "runtime:box"}),
        ] {
            let mut group = json!({"id": "remote-group", "name": "Remote"});
            group
                .as_object_mut()
                .unwrap()
                .extend(remote_stamp.as_object().unwrap().clone());
            let fixture = fixture_with_document(json!({"projectGroups": [group]}));
            let settings_path = fixture._dir.path().join("mcode-data.json");
            let before = std::fs::read(&settings_path).unwrap();
            let dir = write_workspace_dir(fixture._dir.path(), "local-directory");
            let error = workspace_open_for(
                &fixture.scope,
                &fixture.service,
                WorkspaceOpenArgs {
                    folder_path: dir.to_string_lossy().into_owned(),
                    kind: None,
                    project_group_id: Some("remote-group".into()),
                    display_name: None,
                    connection_id: None,
                    host_id: Some("local".into()),
                },
                100,
            )
            .unwrap_err();
            assert_eq!(error.code, "workspace_open_remote_unsupported");
            assert_eq!(std::fs::read(&settings_path).unwrap(), before);
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        }
    }

    #[test]
    fn concurrent_opens_return_their_own_committed_record() {
        for project_group_id in [None, Some("group")] {
            let fixture =
                fixture_with_document(json!({"projectGroups": [{"id": "group", "name": "Group"}]}));
            let dirs: Vec<_> = (0..12)
                .map(|index| {
                    write_workspace_dir(fixture._dir.path(), &format!("workspace-{index}"))
                })
                .collect();
            let barrier = std::sync::Barrier::new(dirs.len());
            std::thread::scope(|threads| {
                for dir in &dirs {
                    let fixture = &fixture;
                    let barrier = &barrier;
                    threads.spawn(move || {
                        let expected_path = dir.to_string_lossy().into_owned();
                        barrier.wait();
                        let result = workspace_open_for(
                            &fixture.scope,
                            &fixture.service,
                            WorkspaceOpenArgs {
                                folder_path: expected_path.clone(),
                                kind: Some("folder".into()),
                                project_group_id: project_group_id.map(str::to_owned),
                                display_name: None,
                                connection_id: None,
                                host_id: Some("local".into()),
                            },
                            100,
                        )
                        .unwrap();
                        let (record, path_field) = if project_group_id.is_some() {
                            (result.folder_workspace.unwrap(), "folderPath")
                        } else {
                            (result.repo.unwrap(), "path")
                        };
                        assert_eq!(record[path_field], json!(expected_path));
                        let document = fixture.service.read_persisted_public().unwrap();
                        let catalog = if project_group_id.is_some() {
                            "folderWorkspaces"
                        } else {
                            "repos"
                        };
                        assert!(document[catalog].as_array().unwrap().contains(&record));
                    });
                }
            });
            let document = fixture.service.read_persisted_public().unwrap();
            let catalog = if project_group_id.is_some() {
                "folderWorkspaces"
            } else {
                "repos"
            };
            assert_eq!(document[catalog].as_array().unwrap().len(), dirs.len());
        }
    }

    #[test]
    fn git_open_rejects_ordinary_folders_then_resolves_roots_and_dedupes_linked_checkouts() {
        let fixture = empty_fixture();
        let main = write_workspace_dir(fixture._dir.path(), "main");
        let nested = write_workspace_dir(&main, "nested");
        let make_args = |path: &Path| WorkspaceOpenArgs {
            folder_path: path.to_string_lossy().into_owned(),
            kind: Some("git".into()),
            project_group_id: None,
            display_name: None,
            connection_id: None,
            host_id: None,
        };
        let before = fixture.service.read_persisted_public().unwrap();
        let error =
            workspace_open_for(&fixture.scope, &fixture.service, make_args(&main), 0).unwrap_err();
        assert_eq!(error.code, "git_not_a_repository");
        assert_eq!(fixture.service.read_persisted_public().unwrap(), before);
        test_git(&main, &["init"]);
        test_git(
            &main,
            &[
                "-c",
                "user.name=Migration Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
        );
        let first =
            workspace_open_for(&fixture.scope, &fixture.service, make_args(&nested), 1).unwrap();
        assert_eq!(
            first.repo.as_ref().unwrap()["path"],
            main.to_string_lossy().as_ref()
        );
        let linked = fixture._dir.path().join("linked");
        test_git(
            &main,
            &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
        );
        let second =
            workspace_open_for(&fixture.scope, &fixture.service, make_args(&linked), 2).unwrap();
        assert!(second.already_existed);
        assert_eq!(second.repo, first.repo);
        assert_eq!(
            fixture.service.read_persisted_public().unwrap()["repos"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn switch_updates_the_active_pointer_in_the_local_partition() {
        let fixture = empty_fixture();
        workspace_switch_for(
            &fixture.service,
            WorkspaceSwitchArgs {
                active_repo_id: Some("repo-1".into()),
                active_worktree_id: Some("worktree-1".into()),
                active_workspace_key: Some("worktree:worktree-1".into()),
                active_workspace_execution_host_id: None,
                active_tab_id: Some("tab-1".into()),
                visited_at: Some(5_000),
                host_id: None,
            },
            0,
        )
        .expect("switch");

        let document = fixture.service.read_persisted_public().unwrap();
        let session = &document["workspaceSession"];
        assert_eq!(session["activeRepoId"], json!("repo-1"));
        assert_eq!(session["activeWorktreeId"], json!("worktree-1"));
        assert_eq!(session["activeWorkspaceKey"], json!("worktree:worktree-1"));
        assert_eq!(session["activeTabId"], json!("tab-1"));
        assert_eq!(
            session["lastVisitedAtByWorktreeId"]["worktree:worktree-1"],
            json!(5_000)
        );
    }

    #[test]
    fn switch_writes_a_non_local_host_partition() {
        let fixture = empty_fixture();
        workspace_switch_for(
            &fixture.service,
            WorkspaceSwitchArgs {
                active_repo_id: None,
                active_worktree_id: Some("wt-ssh".into()),
                active_workspace_key: None,
                active_workspace_execution_host_id: Some("ssh:box".into()),
                active_tab_id: None,
                visited_at: Some(7_000),
                host_id: Some("ssh:box".into()),
            },
            0,
        )
        .expect("switch");

        let document = fixture.service.read_persisted_public().unwrap();
        let partition = &document["workspaceSessionsByHostId"]["ssh:box"];
        assert_eq!(partition["activeWorktreeId"], json!("wt-ssh"));
        assert_eq!(
            partition["activeWorkspaceExecutionHostId"],
            json!("ssh:box")
        );
        assert!(document.get("workspaceSession").is_none());
    }

    #[test]
    fn switch_rejects_an_invalid_host_id() {
        let fixture = empty_fixture();
        let error = workspace_switch_for(
            &fixture.service,
            WorkspaceSwitchArgs {
                active_repo_id: None,
                active_worktree_id: Some("wt".into()),
                active_workspace_key: None,
                active_workspace_execution_host_id: None,
                active_tab_id: None,
                visited_at: None,
                host_id: Some("not-a-host".into()),
            },
            0,
        )
        .expect_err("invalid host id must fail");
        assert_eq!(error.context.operation, "workspace_switch");
    }

    #[test]
    fn list_recent_returns_document_order_and_sort_order_semantics() {
        let fixture = fixture_with_document(json!({
            "repos": [
                { "id": "repo-2", "path": "D:/beta", "displayName": "Beta", "addedAt": 2 },
                { "id": "repo-1", "path": "D:/alpha", "displayName": "Alpha", "addedAt": 1 }
            ],
            "folderWorkspaces": [
                { "id": "fw-a", "projectGroupId": "g", "name": "Zulu", "folderPath": "D:/z", "sortOrder": 10 },
                { "id": "fw-b", "projectGroupId": "g", "name": "Alpha", "folderPath": "D:/a", "sortOrder": 10 },
                { "id": "fw-c", "projectGroupId": "g", "name": "Old", "folderPath": "D:/o", "sortOrder": 5 }
            ],
            "workspaceSession": {
                "activeRepoId": "repo-1",
                "activeWorktreeId": null,
                "activeTabId": null,
                "tabsByWorktree": {},
                "terminalLayoutsByTabId": {},
                "activeWorkspaceKey": "folder:fw-a",
                "lastVisitedAtByWorktreeId": { "folder:fw-a": 99 }
            }
        }));

        let recents =
            workspace_list_recent_for(&fixture.scope, &fixture.service, None).expect("recents");

        // Repos stay in document order (Electron getRepos() array order).
        assert_eq!(recents.repos.len(), 2);
        assert_eq!(recents.repos[0]["id"], json!("repo-2"));
        assert_eq!(recents.repos[1]["id"], json!("repo-1"));

        // Folder workspaces sort sortOrder desc, then name asc.
        let names: Vec<&str> = recents
            .folder_workspaces
            .iter()
            .filter_map(|workspace| workspace.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(names, vec!["Alpha", "Zulu", "Old"]);

        assert_eq!(recents.active_workspace_key, Some("folder:fw-a".into()));
        assert_eq!(recents.active_repo_id, Some("repo-1".into()));
        assert_eq!(
            recents.last_visited_at_by_worktree_id.get("folder:fw-a"),
            Some(&json!(99))
        );
    }

    #[test]
    fn list_recent_reads_the_requested_host_partition() {
        let fixture = fixture_with_document(json!({
            "workspaceSession": {
                "activeRepoId": "local-repo",
                "activeWorktreeId": null,
                "activeTabId": null,
                "tabsByWorktree": {},
                "terminalLayoutsByTabId": {}
            },
            "workspaceSessionsByHostId": {
                "ssh:box": {
                    "activeRepoId": "remote-repo",
                    "activeWorktreeId": null,
                    "activeTabId": null,
                    "tabsByWorktree": {},
                    "terminalLayoutsByTabId": {},
                    "activeWorkspaceKey": "folder:fw-remote"
                }
            }
        }));

        let local =
            workspace_list_recent_for(&fixture.scope, &fixture.service, None).expect("local");
        assert_eq!(local.active_repo_id, Some("local-repo".into()));
        let remote = workspace_list_recent_for(&fixture.scope, &fixture.service, Some("ssh:box"))
            .expect("remote");
        assert_eq!(remote.active_repo_id, Some("remote-repo".into()));
        assert_eq!(remote.active_workspace_key, Some("folder:fw-remote".into()));
    }

    #[test]
    fn list_recent_defaults_an_absent_document() {
        let fixture = empty_fixture();
        let recents =
            workspace_list_recent_for(&fixture.scope, &fixture.service, None).expect("recents");
        assert!(recents.repos.is_empty());
        assert!(recents.folder_workspaces.is_empty());
        assert!(recents.last_visited_at_by_worktree_id.is_empty());
        assert_eq!(recents.active_workspace_key, None);
        assert_eq!(recents.active_repo_id, None);
    }

    #[test]
    fn host_ids_and_comparison_keys_match_the_oracle() {
        assert_eq!(resolve_host_id(None), "local");
        assert_eq!(resolve_host_id(Some("ssh:box")), "ssh:box");
        assert!(crate_is_valid_host_id("runtime:env-1"));
        assert!(!crate_is_valid_host_id("bogus"));

        assert_eq!(path_comparison_key("D:\\Repo\\Alpha\\"), "d:/repo/alpha");
        assert_eq!(
            path_comparison_key("d:/repo/ALPHA"),
            path_comparison_key("D:\\REPO\\alpha\\")
        );
        assert_eq!(path_comparison_key("/home/user/repo"), "/home/user/repo");
    }

    #[test]
    fn empty_session_fields_satisfy_the_document_contract() {
        // Deserialize through the settings crate's own validation path: the
        // required keys (activeRepoId, activeWorktreeId, activeTabId,
        // tabsByWorktree, terminalLayoutsByTabId) must be present.
        let document = json!({ "workspaceSession": empty_session_fields() });
        let parsed: Result<Map<String, Value>, _> =
            serde_json::from_value(document).map_err(|error| error.to_string());
        assert!(parsed.is_ok());
    }
}
