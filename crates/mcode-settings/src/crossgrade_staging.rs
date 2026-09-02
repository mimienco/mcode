//! Crossgrade staging: read the last Electron installation's settings
//! read-only, build a versioned migration plan, stage it to a resumable
//! snapshot, verify the snapshot, and commit it into the Tauri app-data
//! directory — without ever modifying the Electron source tree.
//!
//! Invariants (docs/migration/DATA_AND_SETTINGS_MIGRATION.md):
//! - the Electron location is never changed or deleted; every function here
//!   opens source files read-only,
//! - migration is versioned (`plan_version`), idempotent, and resumable,
//! - progress records hold no key values, no paths outside the two roots,
//!   and no tokens,
//! - telemetry consent is copied exactly as found and never defaulted
//!   (absent stays absent),
//! - `ssh-host-keys.json` holds TOFU *public* host keys (fingerprinted host
//!   key blobs the user explicitly accepted, `ssh-host-key-store.ts:22-31`) —
//!   not secrets — so copying it byte-for-byte is safe; secrets migration
//!   stays out of scope entirely.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Plan schema version. Bump when the extracted-key set or staging layout
/// changes; `verify` rejects plans whose recorded version differs.
pub const CROSSGRADE_PLAN_VERSION: u32 = 1;

/// Original settings document, read-only.
pub const ELECTRON_SETTINGS_FILE: &str = "mcode-data.json";
/// TOFU public host-key store (see module docs: public keys, safe to copy).
pub const ELECTRON_HOST_KEYS_FILE: &str = "ssh-host-keys.json";
/// Original managed agent-hook installs directory (presence check only).
pub const ELECTRON_AGENT_HOOKS_DIR: &str = "agent-hooks";
/// Non-sensitive progress record, written into each staging dir.
pub const PROGRESS_FILE: &str = "crossgrade-progress.json";

// ─── Key mapping table ──────────────────────────────────────────────────────
//
// The Electron `mcode-data.json` and the Tauri `mcode-data.json` share the
// same document layout (`settings`, `ui`, `repos`, `projectGroups`,
// `folderWorkspaces`, …) because the Tauri SettingsService was written
// against the original store shape (`src/main/persistence/loading-store/
// store.ts:1251-1267`). The mapping below therefore copies the consumed
// key paths verbatim; each entry names the SettingsService consumer that
// reads it. If a future rename diverges the two shapes, only this table
// changes.

/// Settings key paths the rewrite's SettingsService consumes, with the
/// original Electron key path in the mapping table. Values are copied
/// verbatim — never re-shaped, never defaulted.
pub const MAPPED_SETTING_KEYS: [(&str, &str); 8] = [
    // "settings" → SettingsService::desktop_window_preferences
    //   .minimize_to_tray_on_close (lib.rs:268-271)
    (
        "settings.minimizeToTrayOnClose",
        "settings.minimizeToTrayOnClose",
    ),
    // "settings" → SettingsService::local_filesystem_roots fallback
    //   workspaceDir (lib.rs:465-471) + insert_worktree_base_root
    //   global_configured (lib.rs:545-549)
    ("settings.workspaceDir", "settings.workspaceDir"),
    // "ui" → SettingsService::desktop_window_preferences .bounds/.maximized
    //   (lib.rs:260-267) and persist_window_placement (lib.rs:315-332)
    ("ui.windowBounds", "ui.windowBounds"),
    ("ui.windowMaximized", "ui.windowMaximized"),
    // workspace-history documents → SettingsService::local_filesystem_roots
    //   (object_array / record_is_local, lib.rs:447-517; preserves canonical
    //   paths, Windows/WSL translation, and missing-workspace behavior)
    ("repos", "repos"),
    ("projectGroups", "projectGroups"),
    ("folderWorkspaces", "folderWorkspaces"),
    // telemetry consent → copied EXACTLY as found, never defaulted.
    //   The original stores `telemetry.optedIn: boolean | null` under
    //   `settings.telemetry` (src/shared/global-settings-types.ts:454-461;
    //   consent resolver src/main/telemetry/consent.ts:100-110). Absent
    //   stays absent — crossgrading must not broaden collection.
    ("settings.telemetry", "settings.telemetry"),
];

#[derive(Debug, PartialEq)]
pub enum CrossgradeError {
    /// The Electron settings document does not exist.
    SourceMissing { path: PathBuf },
    /// The settings document exists but is not readable.
    SourceUnreadable { path: PathBuf, message: String },
    /// The settings document exists but is not valid JSON.
    SourceCorrupt { path: PathBuf, message: String },
    /// The top-level settings document is not a JSON object.
    SourceShapeInvalid { path: PathBuf },
    /// A value recorded under a mapped key changed between plan, stage,
    /// or verify steps (or the staged copy lost one).
    VerifyFailed { key: Option<String>, reason: String },
    /// Staging/commit I/O failed.
    Io { path: PathBuf, message: String },
    /// The staged snapshot's plan version is not `CROSSGRADE_PLAN_VERSION`.
    UnsupportedPlanVersion { found: u32 },
    /// The plan hash recorded in staging does not match the plan at hand —
    /// the staging dir belongs to a different migration attempt.
    PlanHashMismatch { staged: String, expected: String },
}

impl CrossgradeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SourceMissing { .. } => "crossgrade_source_missing",
            Self::SourceUnreadable { .. } | Self::Io { .. } => "crossgrade_stage_failed",
            Self::SourceCorrupt { .. } | Self::SourceShapeInvalid { .. } => {
                "crossgrade_source_corrupt"
            }
            Self::VerifyFailed { .. } => "crossgrade_verify_failed",
            Self::UnsupportedPlanVersion { .. } => "crossgrade_plan_version_unsupported",
            Self::PlanHashMismatch { .. } => "crossgrade_plan_hash_mismatch",
        }
    }
}

impl std::fmt::Display for CrossgradeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceMissing { path } => {
                write!(
                    formatter,
                    "no Electron settings document at {}",
                    path.display()
                )
            }
            Self::SourceUnreadable { path, message } => {
                write!(formatter, "cannot read {}: {message}", path.display())
            }
            Self::SourceCorrupt { path, message } => {
                write!(formatter, "{} is not valid JSON: {message}", path.display())
            }
            Self::SourceShapeInvalid { path } => {
                write!(formatter, "{} is not a JSON object", path.display())
            }
            Self::VerifyFailed { key, reason } => match key {
                Some(key) => write!(formatter, "verification failed for {key}: {reason}"),
                None => write!(formatter, "verification failed: {reason}"),
            },
            Self::Io { path, message } => {
                write!(formatter, "I/O failure at {}: {message}", path.display())
            }
            Self::UnsupportedPlanVersion { found } => {
                write!(
                    formatter,
                    "staged plan version {found} is not supported (expected {CROSSGRADE_PLAN_VERSION})"
                )
            }
            Self::PlanHashMismatch { staged, expected } => {
                write!(
                    formatter,
                    "staging dir holds plan hash {staged}, expected {expected}"
                )
            }
        }
    }
}

impl std::error::Error for CrossgradeError {}

// ─── Plan ───────────────────────────────────────────────────────────────────

/// Presence-only facts (no values — these go into the progress record).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePresence {
    pub host_keys_present: bool,
    pub agent_hooks_present: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossgradePlan {
    pub plan_version: u32,
    /// The Electron userData dir the plan was built from.
    pub source_root: PathBuf,
    /// Epoch millis supplied by the caller (keeps the plan deterministic
    /// for tests and hashing).
    pub created_at: i64,
    /// Extracted settings values keyed by target key path. Absent keys are
    /// absent from the map — telemetry consent absent in the source stays
    /// absent here.
    pub settings: BTreeMap<String, Value>,
    pub source_presence: SourcePresence,
}

/// FNV-1a 64-bit over stable serialization of the plan. Fast, dependency-
/// free, and only ever used to match a staging dir to its plan — never as a
/// security primitive.
fn plan_hash(plan: &CrossgradePlan) -> String {
    let canonical = serde_json::to_vec(plan).expect("plan serialization is infallible");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

impl CrossgradePlan {
    pub fn hash(&self) -> String {
        plan_hash(self)
    }
}

/// Read the Electron settings document read-only and extract exactly the
/// keys in `MAPPED_SETTING_KEYS`. `now_ms` is caller-supplied so plans are
/// reproducible.
pub fn build_plan(
    electron_root: &Path,
    target_dir: &Path,
    now_ms: i64,
) -> Result<CrossgradePlan, CrossgradeError> {
    // target_dir is validated here so a bad destination fails at plan time,
    // before anything is read or staged.
    if target_dir.as_os_str().is_empty() {
        return Err(CrossgradeError::Io {
            path: target_dir.to_path_buf(),
            message: "target directory is empty".into(),
        });
    }

    let settings_path = electron_root.join(ELECTRON_SETTINGS_FILE);
    let bytes = match fs::read(&settings_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(CrossgradeError::SourceMissing {
                path: settings_path,
            });
        }
        Err(error) => {
            return Err(CrossgradeError::SourceUnreadable {
                path: settings_path.clone(),
                message: error.to_string(),
            });
        }
    };
    let document: Value =
        serde_json::from_slice(&bytes).map_err(|error| CrossgradeError::SourceCorrupt {
            path: settings_path.clone(),
            message: error.to_string(),
        })?;
    let root = document
        .as_object()
        .ok_or(CrossgradeError::SourceShapeInvalid {
            path: settings_path.clone(),
        })?;

    let mut settings = BTreeMap::new();
    for (source_path, target_path) in MAPPED_SETTING_KEYS {
        let value = extract_path(root, source_path);
        // Values are copied verbatim. Missing keys are recorded as missing —
        // build_plan never invents a default for any mapped key.
        if let Some(value) = value {
            settings.insert(target_path.to_owned(), value);
        }
    }

    let source_presence = SourcePresence {
        host_keys_present: electron_root.join(ELECTRON_HOST_KEYS_FILE).is_file(),
        agent_hooks_present: electron_root.join(ELECTRON_AGENT_HOOKS_DIR).is_dir(),
    };

    Ok(CrossgradePlan {
        plan_version: CROSSGRADE_PLAN_VERSION,
        source_root: electron_root.to_path_buf(),
        created_at: now_ms,
        settings,
        source_presence,
    })
}

/// Resolve a dotted key path (`settings.telemetry`) against the parsed
/// document. Returns `None` when any segment is missing — absence is
/// preserved, not defaulted.
fn extract_path(root: &serde_json::Map<String, Value>, path: &str) -> Option<Value> {
    let mut segments = path.split('.');
    let mut value = root.get(segments.next()?)?;
    for segment in segments {
        value = value.get(segment)?;
    }
    Some(value.clone())
}

// ─── Stage ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct StagedCrossgrade {
    /// The per-attempt staging directory under target_dir.
    pub dir: PathBuf,
    pub plan_hash: String,
    pub plan: CrossgradePlan,
}

/// Where staged artifacts live inside a staging dir.
pub const STAGED_SETTINGS_FILE: &str = "staged-settings.json";
pub const STAGED_HOST_KEYS_FILE: &str = "staged-ssh-host-keys.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StagedSettingsDocument {
    plan_version: u32,
    plan_hash: String,
    /// Extracted settings keyed by target key path (values verbatim).
    settings: BTreeMap<String, Value>,
    source_presence: SourcePresence,
}

/// Find an existing staging dir for `plan_hash`, or allocate a fresh
/// `crossgrade-staging-<timestamp>` dir. Reusing an existing dir with a
/// matching hash is what makes stage/verify/commit resumable.
fn staging_dir_for(
    target_dir: &Path,
    plan_hash: &str,
    now_ms: i64,
) -> Result<PathBuf, CrossgradeError> {
    if let Ok(entries) = fs::read_dir(target_dir) {
        let mut candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("crossgrade-staging-"))
            })
            .collect();
        candidates.sort();
        for candidate in candidates {
            if existing_plan_hash(&candidate).as_deref() == Some(plan_hash) {
                return Ok(candidate);
            }
        }
    }
    let dir = target_dir.join(format!("crossgrade-staging-{now_ms}"));
    ensure_dir(&dir)?;
    Ok(dir)
}

/// Read the plan hash recorded in an existing staging dir, if any.
fn existing_plan_hash(dir: &Path) -> Option<String> {
    let bytes = fs::read(dir.join(STAGED_SETTINGS_FILE)).ok()?;
    let document: StagedSettingsDocument = serde_json::from_slice(&bytes).ok()?;
    Some(document.plan_hash)
}

fn ensure_dir(dir: &Path) -> Result<(), CrossgradeError> {
    fs::create_dir_all(dir).map_err(|error| CrossgradeError::Io {
        path: dir.to_path_buf(),
        message: error.to_string(),
    })
}

/// Write `staged-settings.json` (+ `crossgrade-progress.json`) and copy the
/// host-key file byte-for-byte when the source had one. Re-running with the
/// same plan reuses and completes the same staging dir; a partial prior run
/// is simply finished.
pub fn stage(
    plan: &CrossgradePlan,
    target_dir: &Path,
    now_ms: i64,
) -> Result<StagedCrossgrade, CrossgradeError> {
    if plan.plan_version != CROSSGRADE_PLAN_VERSION {
        return Err(CrossgradeError::UnsupportedPlanVersion {
            found: plan.plan_version,
        });
    }
    let hash = plan.hash();
    let dir = staging_dir_for(target_dir, &hash, now_ms)?;
    ensure_dir(&dir)?;

    // Host keys are TOFU *public* keys (module docs) — copy bytes exactly.
    let host_keys_source = plan.source_root.join(ELECTRON_HOST_KEYS_FILE);
    if plan.source_presence.host_keys_present {
        let bytes = read_source_bytes(&host_keys_source)?;
        write_bytes(&dir.join(STAGED_HOST_KEYS_FILE), &bytes)?;
    }

    let document = StagedSettingsDocument {
        plan_version: plan.plan_version,
        plan_hash: hash.clone(),
        settings: plan.settings.clone(),
        source_presence: plan.source_presence.clone(),
    };
    let serialized = serde_json::to_vec_pretty(&document).map_err(|error| CrossgradeError::Io {
        path: dir.join(STAGED_SETTINGS_FILE),
        message: format!("serialize staged settings: {error}"),
    })?;
    write_bytes(&dir.join(STAGED_SETTINGS_FILE), &serialized)?;

    write_progress(&dir, &hash, "staged", plan)?;

    Ok(StagedCrossgrade {
        dir,
        plan_hash: hash,
        plan: plan.clone(),
    })
}

fn read_source_bytes(path: &Path) -> Result<Vec<u8>, CrossgradeError> {
    fs::read(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            CrossgradeError::SourceMissing {
                path: path.to_path_buf(),
            }
        } else {
            CrossgradeError::SourceUnreadable {
                path: path.to_path_buf(),
                message: error.to_string(),
            }
        }
    })
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), CrossgradeError> {
    fs::write(path, bytes).map_err(|error| CrossgradeError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// Non-sensitive progress record: plan hash, phase, timestamps, counts. No
/// key values, no paths outside the two roots, no tokens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressRecord {
    pub plan_version: u32,
    pub plan_hash: String,
    pub phase: String,
    pub updated_at: i64,
    pub extracted_settings_count: u32,
    pub host_keys_copied: bool,
}

fn write_progress(
    dir: &Path,
    plan_hash: &str,
    phase: &str,
    plan: &CrossgradePlan,
) -> Result<(), CrossgradeError> {
    let record = ProgressRecord {
        plan_version: plan.plan_version,
        plan_hash: plan_hash.to_owned(),
        phase: phase.to_owned(),
        updated_at: plan.created_at,
        extracted_settings_count: plan.settings.len() as u32,
        host_keys_copied: plan.source_presence.host_keys_present,
    };
    let serialized = serde_json::to_vec_pretty(&record).map_err(|error| CrossgradeError::Io {
        path: dir.join(PROGRESS_FILE),
        message: format!("serialize progress: {error}"),
    })?;
    write_bytes(&dir.join(PROGRESS_FILE), &serialized)
}

// ─── Verify ─────────────────────────────────────────────────────────────────

/// One checked item in the verify report. Carries names/counts only — never
/// a value — so the report stays non-sensitive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyItem {
    pub name: String,
    pub ok: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub plan_version: u32,
    pub plan_hash: String,
    pub settings_checked: u32,
    pub host_keys_byte_identical: bool,
    pub telemetry_preserved: bool,
    pub items: Vec<VerifyItem>,
}

impl VerifyReport {
    pub fn is_ok(&self) -> bool {
        self.items.iter().all(|item| item.ok)
    }
}

/// Re-parse the staged snapshot and assert:
/// - the staged plan version is supported and the hash matches the plan,
/// - every key the plan extracted is present in the staged document with an
///   identical value,
/// - the host-keys copy is byte-identical to the source file,
/// - telemetry consent equals the source value exactly (absent stays absent).
pub fn verify(staged: &StagedCrossgrade) -> Result<VerifyReport, CrossgradeError> {
    verify_against_plan(staged, &staged.plan)
}

fn verify_against_plan(
    staged: &StagedCrossgrade,
    plan: &CrossgradePlan,
) -> Result<VerifyReport, CrossgradeError> {
    let bytes =
        fs::read(staged.dir.join(STAGED_SETTINGS_FILE)).map_err(|error| CrossgradeError::Io {
            path: staged.dir.join(STAGED_SETTINGS_FILE),
            message: error.to_string(),
        })?;
    let document: StagedSettingsDocument =
        serde_json::from_slice(&bytes).map_err(|error| CrossgradeError::VerifyFailed {
            key: None,
            reason: format!("staged settings document is corrupt: {error}"),
        })?;
    if document.plan_version != CROSSGRADE_PLAN_VERSION {
        return Err(CrossgradeError::UnsupportedPlanVersion {
            found: document.plan_version,
        });
    }
    if document.plan_hash != plan.hash() {
        return Err(CrossgradeError::PlanHashMismatch {
            staged: document.plan_hash,
            expected: plan.hash(),
        });
    }

    let mut items = Vec::new();

    // Every planned key must be present and identical.
    for (key, expected) in &plan.settings {
        let ok = document
            .settings
            .get(key)
            .is_some_and(|staged_value| staged_value == expected);
        items.push(VerifyItem {
            name: format!("setting:{key}"),
            ok,
        });
        if !ok {
            return Err(CrossgradeError::VerifyFailed {
                key: Some(key.clone()),
                reason: "staged value missing or diverged from the plan".into(),
            });
        }
    }

    // Telemetry consent equality with the live source, exactly as found.
    // (Both absent → absent: nothing was defaulted into existence.)
    let telemetry_ok = telemetry_matches_source(plan);
    items.push(VerifyItem {
        name: "telemetry:consent_matches_source".into(),
        ok: telemetry_ok,
    });
    if !telemetry_ok {
        return Err(CrossgradeError::VerifyFailed {
            key: Some("settings.telemetry".into()),
            reason: "staged telemetry consent diverged from the source".into(),
        });
    }

    // Host keys: byte-identical copy, or both sides absent.
    let host_keys_source = plan.source_root.join(ELECTRON_HOST_KEYS_FILE);
    let staged_host_keys = staged.dir.join(STAGED_HOST_KEYS_FILE);
    let host_keys_ok = if plan.source_presence.host_keys_present {
        let source_bytes = read_source_bytes(&host_keys_source)?;
        let staged_bytes = read_source_bytes(&staged_host_keys)?;
        source_bytes == staged_bytes
    } else {
        !staged_host_keys.exists()
    };
    items.push(VerifyItem {
        name: "host-keys:byte_identical".into(),
        ok: host_keys_ok,
    });
    if !host_keys_ok {
        return Err(CrossgradeError::VerifyFailed {
            key: Some(ELECTRON_HOST_KEYS_FILE.into()),
            reason: "staged host keys are missing or differ from the source".into(),
        });
    }

    let report = VerifyReport {
        plan_version: document.plan_version,
        plan_hash: document.plan_hash,
        settings_checked: plan.settings.len() as u32,
        host_keys_byte_identical: host_keys_ok,
        telemetry_preserved: telemetry_ok,
        items,
    };
    write_progress(&staged.dir, &staged.plan_hash, "verified", plan)?;
    Ok(report)
}

/// Compare the staged telemetry value against the current source document.
/// Absence on both sides is equality (the absent-stays-absent invariant).
fn telemetry_matches_source(plan: &CrossgradePlan) -> bool {
    let staged_value = plan.settings.get("settings.telemetry");
    match read_source_document(&plan.source_root.join(ELECTRON_SETTINGS_FILE)) {
        Some(document) => {
            let source_value = document
                .as_object()
                .and_then(|root| extract_path(root, "settings.telemetry"));
            staged_value == source_value.as_ref()
        }
        // Source vanished mid-migration: only the empty-telemetry case is
        // still verifiable.
        None => staged_value.is_none(),
    }
}

fn read_source_document(path: &Path) -> Option<Value> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ─── Commit ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    /// `true` on a fresh commit, `false` when the commit was an idempotent
    /// no-op because the target already holds this plan's data.
    pub wrote: bool,
    pub migrated_keys: u32,
    pub host_keys_copied: bool,
}

/// Commit the verified snapshot into `target` (the Tauri app-data dir):
/// - `target/mcode-data.json` — merged with existing content (settings are
///   an overlay on whatever the app already persisted),
/// - `target/ssh-host-keys.json` — byte-for-byte copy when staged.
///
/// Atomicity: write-then-rename via a temp file, with Windows
/// remove-then-rename (mirrors mcode-app::runtime_metadata::write_atomic,
/// where a plain rename over an existing file fails on Windows).
///
/// Idempotence: if the target document already contains exactly the planned
/// values, commit is a no-op success (`wrote: false`) and the staging dir is
/// left for the caller to discard.
pub fn commit(staged: &StagedCrossgrade, target: &Path) -> Result<CommitOutcome, CrossgradeError> {
    let report = verify_against_plan(staged, &staged.plan)?;
    if !report.is_ok() {
        return Err(CrossgradeError::VerifyFailed {
            key: None,
            reason: "refusing to commit an unverified snapshot".into(),
        });
    }

    let settings_target = target.join(ELECTRON_SETTINGS_FILE);
    let existing = read_source_document(&settings_target)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    let planned_values: Vec<(&str, &Value)> = staged
        .plan
        .settings
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();

    // Keys are dotted TARGET paths (`settings.telemetry`); they name a
    // position inside the document, not a literal top-level key. Reads and
    // writes both go through the pointer helpers so a nested mapping lands
    // nested (commit_merges/happy_path/telemetry tests pin this) while
    // single-segment keys (`repos`, `projectGroups`, `folderWorkspaces`)
    // stay top-level.
    fn pointer_at<'a>(
        document: &'a serde_json::Map<String, Value>,
        path: &str,
    ) -> Option<&'a Value> {
        let mut segments = path.split('.');
        let first = segments.next()?;
        let mut value = document.get(first)?;
        for segment in segments {
            value = value.get(segment)?;
        }
        Some(value)
    }

    /// Insert `value` at the dotted path, reusing existing intermediate
    /// objects (an existing `settings.theme` survives a
    /// `settings.telemetry` overlay) and creating them when missing.
    fn insert_at(document: &mut serde_json::Map<String, Value>, path: &str, value: Value) {
        let mut segments = path.split('.');
        let first = segments.next().expect("mapped keys are never empty");
        let mut cursor = document
            .entry(first.to_owned())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        for segment in segments {
            if !cursor.is_object() {
                *cursor = Value::Object(serde_json::Map::new());
            }
            cursor = cursor
                .as_object_mut()
                .expect("just normalized to an object")
                .entry(segment.to_owned())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
        }
        *cursor = value;
    }

    // Idempotence: nothing to do if every planned value is already present
    // and identical in the target document (read through the dotted path).
    let already_applied = planned_values
        .iter()
        .all(|(key, value)| pointer_at(&existing, key).is_some_and(|current| current == *value))
        && staged
            .plan
            .source_presence
            .host_keys_present
            .implies(target.join(ELECTRON_HOST_KEYS_FILE).is_file());

    if already_applied {
        write_progress(&staged.dir, &staged.plan_hash, "committed", &staged.plan)?;
        return Ok(CommitOutcome {
            wrote: false,
            migrated_keys: staged.plan.settings.len() as u32,
            host_keys_copied: staged.plan.source_presence.host_keys_present,
        });
    }

    let mut merged = existing;
    for (key, value) in &planned_values {
        insert_at(&mut merged, key, (*value).clone());
    }

    // Host keys first: if the settings write then fails, the next attempt
    // re-verifies and completes; the reverse order could leave a settings
    // document referencing host keys that were never copied.
    if staged.plan.source_presence.host_keys_present {
        let staged_bytes = read_source_bytes(&staged.dir.join(STAGED_HOST_KEYS_FILE))?;
        write_atomic(&target.join(ELECTRON_HOST_KEYS_FILE), &staged_bytes)?;
    }

    let serialized = serde_json::to_vec_pretty(&merged).map_err(|error| CrossgradeError::Io {
        path: settings_target.clone(),
        message: format!("serialize merged settings: {error}"),
    })?;
    write_atomic(&settings_target, &serialized)?;

    write_progress(&staged.dir, &staged.plan_hash, "committed", &staged.plan)?;
    Ok(CommitOutcome {
        wrote: true,
        migrated_keys: staged.plan.settings.len() as u32,
        host_keys_copied: staged.plan.source_presence.host_keys_present,
    })
}

/// Tiny helper for the `bool implies bool` truth table used by the
/// idempotence check above.
trait Implies {
    fn implies(self, other: bool) -> bool;
}

impl Implies for bool {
    fn implies(self, other: bool) -> bool {
        !self || other
    }
}

/// Write-then-rename with the Windows remove-then-rename workaround
/// (mcode-app::runtime_metadata::write_atomic pattern).
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CrossgradeError> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => {
            return Err(CrossgradeError::Io {
                path: path.to_path_buf(),
                message: "target path has no parent directory".into(),
            });
        }
    };
    ensure_dir(&parent)?;
    let temp = parent.join(format!(
        "{}.crossgrade-tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| CrossgradeError::Io {
                path: path.to_path_buf(),
                message: "target file name is not valid UTF-8".into(),
            })?
    ));
    fs::write(&temp, bytes).map_err(|error| CrossgradeError::Io {
        path: temp.clone(),
        message: error.to_string(),
    })?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|error| CrossgradeError::Io {
            path: path.to_path_buf(),
            message: format!("remove before rename: {error}"),
        })?;
    }
    fs::rename(&temp, path).map_err(|error| CrossgradeError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// Discard a staging dir after a successful commit (the Tauri command
/// reports `stagingDiscarded` from the result).
pub fn discard(staged: &StagedCrossgrade) -> Result<bool, CrossgradeError> {
    match fs::remove_dir_all(&staged.dir) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(CrossgradeError::Io {
            path: staged.dir.clone(),
            message: error.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    /// Electron-shaped source with every mapped key present.
    fn write_source(root: &Path, document: Value) {
        fs::write(
            root.join(ELECTRON_SETTINGS_FILE),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
    }

    fn sample_document() -> Value {
        json!({
            "settings": {
                "minimizeToTrayOnClose": true,
                "workspaceDir": "D:/work",
                "telemetry": { "optedIn": false, "installId": "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0" }
            },
            "ui": { "windowBounds": { "x": 8, "y": 16, "width": 1280, "height": 720 }, "windowMaximized": false },
            "repos": [{ "id": "r1", "path": "D:/work/app" }],
            "projectGroups": [{ "id": "g1", "parentPath": "D:/work" }],
            "folderWorkspaces": [{ "folderPath": "D:/work/app", "projectGroupId": "g1" }],
            "unrelated": "left-alone"
        })
    }

    fn host_keys_bytes() -> &'static [u8] {
        br#"{"version":1,"entries":[]}"#
    }

    /// Snapshot every byte under `root` (path + contents) for the
    /// source-untouched invariant test. Deterministic order, byte hashes.
    fn snapshot_dir(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut snapshot = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    snapshot.insert(path.clone(), fs::read(&path).unwrap());
                }
            }
        }
        snapshot
    }

    fn assert_planned_values(plan: &CrossgradePlan, document: &Value) {
        assert_eq!(
            plan.settings.get("settings.minimizeToTrayOnClose"),
            document.pointer("/settings/minimizeToTrayOnClose")
        );
        assert_eq!(
            plan.settings.get("settings.workspaceDir"),
            document.pointer("/settings/workspaceDir")
        );
        assert_eq!(
            plan.settings.get("settings.telemetry"),
            document.pointer("/settings/telemetry")
        );
        assert_eq!(
            plan.settings.get("ui.windowBounds"),
            document.pointer("/ui/windowBounds")
        );
        assert_eq!(
            plan.settings.get("ui.windowMaximized"),
            document.pointer("/ui/windowMaximized")
        );
        assert_eq!(plan.settings.get("repos"), document.pointer("/repos"));
        assert_eq!(
            plan.settings.get("projectGroups"),
            document.pointer("/projectGroups")
        );
        assert_eq!(
            plan.settings.get("folderWorkspaces"),
            document.pointer("/folderWorkspaces")
        );
        assert_eq!(plan.settings.len(), 8);
    }

    #[test]
    fn happy_path_plans_stages_verifies_and_commits() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());
        fs::write(
            source.path().join(ELECTRON_HOST_KEYS_FILE),
            host_keys_bytes(),
        )
        .unwrap();
        fs::create_dir_all(source.path().join(ELECTRON_AGENT_HOOKS_DIR)).unwrap();

        let plan = build_plan(source.path(), target.path(), 1_700_000_000_000).unwrap();
        assert_planned_values(&plan, &sample_document());
        assert!(plan.source_presence.host_keys_present);
        assert!(plan.source_presence.agent_hooks_present);
        assert_eq!(plan.plan_version, CROSSGRADE_PLAN_VERSION);

        let staged = stage(&plan, staging.path(), 1_700_000_000_001).unwrap();
        assert!(
            staged
                .dir
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("crossgrade-staging-")
        );

        let report = verify(&staged).unwrap();
        assert!(report.is_ok());
        assert!(report.host_keys_byte_identical);
        assert!(report.telemetry_preserved);
        assert_eq!(report.settings_checked, 8);

        let outcome = commit(&staged, target.path()).unwrap();
        assert!(outcome.wrote);
        assert!(outcome.host_keys_copied);
        assert_eq!(outcome.migrated_keys, 8);

        // Committed target holds the exact planned values.
        let committed: Value =
            serde_json::from_slice(&fs::read(target.path().join(ELECTRON_SETTINGS_FILE)).unwrap())
                .unwrap();
        assert_eq!(
            committed.pointer("/settings/telemetry"),
            sample_document().pointer("/settings/telemetry")
        );
        assert_eq!(
            committed.pointer("/settings/minimizeToTrayOnClose"),
            Some(&json!(true))
        );
        assert_eq!(
            fs::read(target.path().join(ELECTRON_HOST_KEYS_FILE)).unwrap(),
            host_keys_bytes()
        );
        assert!(
            !target
                .path()
                .join(ELECTRON_AGENT_HOOKS_DIR)
                .join(".keep")
                .exists()
        );
    }

    #[test]
    fn agent_hooks_dir_is_presence_checked_only() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());
        fs::create_dir_all(source.path().join(ELECTRON_AGENT_HOOKS_DIR)).unwrap();

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        assert!(plan.source_presence.agent_hooks_present);
        let staged = stage(&plan, staging.path(), 2).unwrap();
        assert!(verify(&staged).unwrap().is_ok());
        // Presence check only — no agent-hooks content is copied into the
        // staging dir or the target.
        assert!(!staged.dir.join(ELECTRON_AGENT_HOOKS_DIR).exists());
        commit(&staged, target.path()).unwrap();
        assert!(!target.path().join(ELECTRON_AGENT_HOOKS_DIR).exists());
    }

    #[test]
    fn missing_source_is_reported_not_defaulted() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        let error = build_plan(source.path(), target.path(), 1).unwrap_err();
        assert_eq!(error.code(), "crossgrade_source_missing");
        assert!(matches!(error, CrossgradeError::SourceMissing { .. }));
    }

    #[test]
    fn corrupt_source_json_is_reported() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        fs::write(source.path().join(ELECTRON_SETTINGS_FILE), b"{not json").unwrap();
        let error = build_plan(source.path(), target.path(), 1).unwrap_err();
        assert_eq!(error.code(), "crossgrade_source_corrupt");
    }

    #[test]
    fn non_object_source_json_is_reported() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        fs::write(source.path().join(ELECTRON_SETTINGS_FILE), b"[1,2,3]").unwrap();
        let error = build_plan(source.path(), target.path(), 1).unwrap_err();
        assert_eq!(error.code(), "crossgrade_source_corrupt");
    }

    #[test]
    fn rerun_with_same_plan_is_idempotent() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged_a = stage(&plan, staging.path(), 100).unwrap();
        commit(&staged_a, target.path()).unwrap();

        // Re-run the whole flow: stage reuses the same dir, verify passes,
        // commit is a no-op.
        let plan_again = build_plan(source.path(), target.path(), 1).unwrap();
        assert_eq!(plan_again.hash(), plan.hash());
        let staged_b = stage(&plan_again, staging.path(), 200).unwrap();
        assert_eq!(staged_b.dir, staged_a.dir);
        assert!(verify(&staged_b).unwrap().is_ok());
        let outcome = commit(&staged_b, target.path()).unwrap();
        assert!(!outcome.wrote, "re-commit must be a no-op success");

        // And exactly one staging dir was created.
        let staging_dirs = fs::read_dir(staging.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("crossgrade-staging-"))
            })
            .count();
        assert_eq!(staging_dirs, 1);
    }

    #[test]
    fn partial_staging_is_resumed_and_completed() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());
        fs::write(
            source.path().join(ELECTRON_HOST_KEYS_FILE),
            host_keys_bytes(),
        )
        .unwrap();

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 100).unwrap();

        // Simulate an interrupted run: staged settings survive, host keys
        // are lost.
        fs::remove_file(staged.dir.join(STAGED_HOST_KEYS_FILE)).unwrap();
        assert!(staged.dir.join(STAGED_SETTINGS_FILE).exists());

        // Re-stage the same plan: the same dir is reused and completed.
        let resumed = stage(&plan, staging.path(), 200).unwrap();
        assert_eq!(resumed.dir, staged.dir);
        assert!(resumed.dir.join(STAGED_HOST_KEYS_FILE).exists());
        assert!(verify(&resumed).unwrap().is_ok());

        let outcome = commit(&resumed, target.path()).unwrap();
        assert!(outcome.wrote);
        assert_eq!(
            fs::read(target.path().join(ELECTRON_HOST_KEYS_FILE)).unwrap(),
            host_keys_bytes()
        );
    }

    #[test]
    fn source_dir_is_byte_identical_after_full_cycle() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());
        fs::write(
            source.path().join(ELECTRON_HOST_KEYS_FILE),
            host_keys_bytes(),
        )
        .unwrap();
        fs::create_dir_all(source.path().join(ELECTRON_AGENT_HOOKS_DIR)).unwrap();
        fs::write(
            source
                .path()
                .join(ELECTRON_AGENT_HOOKS_DIR)
                .join("claude.json"),
            b"{}",
        )
        .unwrap();

        let before = snapshot_dir(source.path());
        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 2).unwrap();
        assert!(verify(&staged).unwrap().is_ok());
        commit(&staged, target.path()).unwrap();
        let after = snapshot_dir(source.path());
        assert_eq!(before, after, "the Electron root must never change");
    }

    #[test]
    fn telemetry_preservation_exact_values() {
        // false, "unset"-style null, true, and absent round-trip exactly.
        for telemetry in [
            json!(null),
            json!(false),
            json!(true),
            json!({ "optedIn": false, "installId": "x" }),
        ] {
            let source = TempDir::new().unwrap();
            let staging = TempDir::new().unwrap();
            let target = TempDir::new().unwrap();
            let mut document = sample_document();
            document["settings"]["telemetry"] = telemetry.clone();
            write_source(source.path(), document);

            let plan = build_plan(source.path(), target.path(), 1).unwrap();
            assert_eq!(plan.settings.get("settings.telemetry"), Some(&telemetry));
            let staged = stage(&plan, staging.path(), 2).unwrap();
            let report = verify(&staged).unwrap();
            assert!(report.telemetry_preserved);
            commit(&staged, target.path()).unwrap();
            let committed: Value = serde_json::from_slice(
                &fs::read(target.path().join(ELECTRON_SETTINGS_FILE)).unwrap(),
            )
            .unwrap();
            assert_eq!(committed.pointer("/settings/telemetry"), Some(&telemetry));
        }
    }

    #[test]
    fn telemetry_absent_stays_absent_and_never_defaults() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        let mut document = sample_document();
        // Source has no telemetry block at all.
        document["settings"]
            .as_object_mut()
            .unwrap()
            .remove("telemetry");
        write_source(source.path(), document);

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        assert!(
            !plan.settings.contains_key("settings.telemetry"),
            "absent consent must stay absent — never defaulted to broader collection"
        );
        assert_eq!(plan.settings.len(), 7);

        let staged = stage(&plan, staging.path(), 2).unwrap();
        let report = verify(&staged).unwrap();
        assert!(report.telemetry_preserved, "absent == absent is equality");

        commit(&staged, target.path()).unwrap();
        let committed: Value =
            serde_json::from_slice(&fs::read(target.path().join(ELECTRON_SETTINGS_FILE)).unwrap())
                .unwrap();
        assert!(committed.pointer("/settings/telemetry").is_none());
    }

    #[test]
    fn verify_detects_tampered_staged_settings() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 2).unwrap();

        // Tamper with one staged value.
        let mut document: StagedSettingsDocument =
            serde_json::from_slice(&fs::read(staged.dir.join(STAGED_SETTINGS_FILE)).unwrap())
                .unwrap();
        document
            .settings
            .insert("settings.workspaceDir".into(), json!("D:/elsewhere"));
        fs::write(
            staged.dir.join(STAGED_SETTINGS_FILE),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();

        let error = verify(&staged).unwrap_err();
        assert_eq!(error.code(), "crossgrade_verify_failed");
    }

    #[test]
    fn verify_detects_missing_staged_file() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 2).unwrap();
        fs::remove_file(staged.dir.join(STAGED_SETTINGS_FILE)).unwrap();

        assert!(matches!(verify(&staged), Err(CrossgradeError::Io { .. })));
    }

    #[test]
    fn commit_merges_with_existing_target_document() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());
        // Target already holds its own state the app wrote after first run.
        fs::write(
            target.path().join(ELECTRON_SETTINGS_FILE),
            serde_json::to_vec_pretty(&json!({ "settings": { "theme": "dark" } })).unwrap(),
        )
        .unwrap();

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 2).unwrap();
        commit(&staged, target.path()).unwrap();

        let committed: Value =
            serde_json::from_slice(&fs::read(target.path().join(ELECTRON_SETTINGS_FILE)).unwrap())
                .unwrap();
        assert_eq!(committed.pointer("/settings/theme"), Some(&json!("dark")));
        assert_eq!(
            committed.pointer("/settings/minimizeToTrayOnClose"),
            Some(&json!(true))
        );
    }

    #[test]
    fn progress_record_is_non_sensitive() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());
        fs::write(
            source.path().join(ELECTRON_HOST_KEYS_FILE),
            host_keys_bytes(),
        )
        .unwrap();

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 2).unwrap();
        commit(&staged, target.path()).unwrap();

        let raw = fs::read_to_string(staged.dir.join(PROGRESS_FILE)).unwrap();
        assert!(raw.contains(&plan.hash()));
        assert!(raw.contains("\"phase\""));
        assert!(raw.contains("\"extractedSettingsCount\""));
        // No paths outside the two roots, no tokens, no key values.
        assert!(!raw.contains("workspaceDir"));
        assert!(!raw.contains("installId"));
        assert!(!raw.contains("optedIn"));
        assert!(!raw.contains("windowBounds"));
        assert!(
            !raw.contains(
                staging
                    .path()
                    .to_str()
                    .unwrap()
                    .split('\\')
                    .next_back()
                    .unwrap_or("zzz-nope")
            )
        );
    }

    #[test]
    fn unsupported_plan_version_is_rejected() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());

        let mut plan = build_plan(source.path(), target.path(), 1).unwrap();
        plan.plan_version = CROSSGRADE_PLAN_VERSION + 1;
        let error = stage(&plan, staging.path(), 2).unwrap_err();
        assert_eq!(error.code(), "crossgrade_plan_version_unsupported");
    }

    #[test]
    fn discard_removes_staging_dir_once() {
        let source = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_source(source.path(), sample_document());

        let plan = build_plan(source.path(), target.path(), 1).unwrap();
        let staged = stage(&plan, staging.path(), 2).unwrap();
        assert!(discard(&staged).unwrap());
        assert!(!staged.dir.exists());
        assert!(!discard(&staged).unwrap(), "second discard is a no-op");
    }
}
