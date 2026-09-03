//! Updater commands beyond the core check/download/install loop
//! (`updater_commands.rs`): app version, nudge/available-update dismissal,
//! the dev-channel release-build picker feed, and the Linux package-recovery
//! surface ported from `orca/src/main/updater.ts` + `orca/src/main/updater-release-builds.ts`
//! (Electron HEAD 5c13ecce).
// Why: in-flight updater-extra slice (release-entry parsing for the
// multi-channel updater) — types wired with the updater.channels batch.
#![allow(dead_code)]

use std::time::Duration;

use mcode_updater::ReleaseChannel;
use serde::Serialize;
use tauri::AppHandle;

use crate::DesktopCommandError;

// ─── App version ─────────────────────────────────────────────────────────────

/// Mirrors the `updater:getVersion` handler (`app.getVersion()`,
/// orca/src/main/window/attach-main-window-services.ts:573).
#[tauri::command]
pub(crate) fn update_get_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

// ─── Nudge / available-update dismissal ──────────────────────────────────────

/// In-memory dismissal state backing `update_dismiss_nudge` and
/// `update_dismiss_available_update`.
///
/// GAP (parity row open): the Electron original persists
/// `pendingUpdateNudgeId` / `dismissedUpdateNudgeId` through the settings
/// store's UI patch (orca/src/main/window/attach-main-window-services.ts:182-194,
/// `store.updateUI({ dismissedUpdateNudgeId })`). The Tauri settings service
/// (`crates/mcode-settings`) has no UI nudge fields, so this state is
/// session-scoped only: a relaunch re-shows a nudge Electron would keep hidden.
#[derive(Default)]
pub struct UpdaterDismissalState {
    nudge: std::sync::Mutex<UpdaterDismissalInner>,
}

#[derive(Default)]
struct UpdaterDismissalInner {
    pending_nudge_id: Option<String>,
    dismissed_nudge_id: Option<String>,
    available_update_dismissed: bool,
}

fn dismissal_lock_error(operation: &'static str) -> DesktopCommandError {
    DesktopCommandError::new(
        "update_state_unavailable",
        operation,
        "updater dismissal state is unavailable".to_owned(),
        "MCode could not record the update dismissal. Try again.",
        None,
        None,
    )
}

/// Port of `dismissNudge` (orca/src/main/updater.ts:2130-2136): moves the
/// pending campaign id into the dismissed slot so `shouldApplyNudge`
/// (orca/src/main/updater-nudge.ts:80-88) stops re-firing it. A no-op when no
/// campaign is pending, exactly like the original.
#[tauri::command]
pub(crate) fn update_dismiss_nudge(
    state: tauri::State<'_, UpdaterDismissalState>,
) -> Result<(), DesktopCommandError> {
    let mut inner = state
        .nudge
        .lock()
        .map_err(|_| dismissal_lock_error("update_dismiss_nudge"))?;
    if let Some(pending) = inner.pending_nudge_id.take() {
        inner.dismissed_nudge_id = Some(pending);
    }
    Ok(())
}

/// Port of `dismissAvailableUpdate` (orca/src/main/updater.ts:2143-2158).
///
/// GAP (parity row open): the original's release-source guard
/// (`activeUpdateSource === 'release' && !isPinnedBuildActive` → early return,
/// updater.ts:2144-2146), the clear-available-context step, the
/// `restoreReleaseUpdateSource()` call, and the trailing `sendStatus({ state:
/// 'idle' })` broadcast (updater.ts:2154-2157) all depend on the
/// local-build/pinned-feed machinery, which does not exist in the Tauri
/// backend. `NativeUpdaterState.service` is module-private
/// (`updater_commands.rs:30-34`), so this command cannot reset the native
/// status to `Idle` without touching `updater_commands.rs`. It records the
/// dismissal flag only; the renderer-side card still unmounts.
#[tauri::command]
pub(crate) fn update_dismiss_available_update(
    state: tauri::State<'_, UpdaterDismissalState>,
) -> Result<(), DesktopCommandError> {
    let mut inner = state
        .nudge
        .lock()
        .map_err(|_| dismissal_lock_error("update_dismiss_available_update"))?;
    inner.available_update_dismissed = true;
    Ok(())
}

// ─── Linux package recovery ──────────────────────────────────────────────────

/// Port of `getLinuxPackageInstallInstructions`
/// (orca/src/main/updater.ts:1987-2012).
///
/// Why this always rejects: the original gates on
/// `getActiveLinuxPackageRecovery()` (updater.ts:1988-1991) — a
/// `LinuxPackageInstallRecovery` card attached to an `error` status after a
/// root-package install needs privilege escalation. The Tauri `UpdateStatus`
/// state machine (`crates/mcode-updater/src/service.rs:60-102`) has no
/// `recovery` field on its `Error` variant and no linux-package variant at
/// all, so no status this backend can produce ever carries recovery. Parity
/// row stays open until the recovery card is ported into `UpdateStatus::Error`.
#[tauri::command]
pub(crate) fn update_get_linux_package_install_instructions() -> Result<(), DesktopCommandError> {
    Err(DesktopCommandError::new(
        "update_linux_package_unavailable",
        "update_get_linux_package_install_instructions",
        "No package install recovery is available.".to_owned(),
        "No package install recovery is available.",
        None,
        None,
    ))
}

/// Port of `showLinuxPackage` (orca/src/main/updater.ts:2014-2028). Same
/// permanent guard as `update_get_linux_package_install_instructions` — see
/// its Why comment. When recovery exists upstream, the reveal goes through the
/// opener plugin on the revalidated cached package path.
#[tauri::command]
pub(crate) fn update_show_linux_package() -> Result<(), DesktopCommandError> {
    Err(DesktopCommandError::new(
        "update_linux_package_unavailable",
        "update_show_linux_package",
        "No package install recovery is available.".to_owned(),
        "No package install recovery is available.",
        None,
        None,
    ))
}

// ─── Release build list (dev channel picker) ─────────────────────────────────

/// Mirrors `ReleaseBuild` from orca/src/shared/release-channel.ts:305-318.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseBuild {
    pub tag: String,
    pub version: String,
    pub channel: String,
    /// The release's GitHub title. Null when absent or just the tag
    /// (orca/src/main/updater-release-builds.ts:66-69).
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub release_url: String,
    pub installer_url: Option<String>,
}

/// Mirrors `ReleaseBuildListResult` from
/// orca/src/shared/update-status-types.ts:86-88. Untagged because the
/// discriminator is the boolean `ok` field, not a string tag.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseBuildListOk {
    ok: bool,
    channel: ReleaseChannel,
    builds: Vec<ReleaseBuild>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseBuildListErr {
    ok: bool,
    channel: ReleaseChannel,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ReleaseBuildListResult {
    Ok(ReleaseBuildListOk),
    Err(ReleaseBuildListErr),
}

fn builds_error(channel: ReleaseChannel, message: String) -> ReleaseBuildListResult {
    ReleaseBuildListResult::Err(ReleaseBuildListErr {
        ok: false,
        channel,
        message,
    })
}

// Constants from orca/src/main/updater-release-builds.ts:14-15.
const FETCH_TIMEOUT_MS: u64 = 8000;
const MAX_LISTED_BUILDS: usize = 100;

/// Mirrors `CHANNEL_RELEASE_REPOS` (orca/src/shared/release-channel.ts:38-44)
/// with the repo constants from release-channel.ts:24-27.
fn repo_for_channel(channel: ReleaseChannel) -> &'static str {
    match channel {
        ReleaseChannel::Stable | ReleaseChannel::Rc => "mcode-ide/mcode",
        ReleaseChannel::Hourly => "mcode-ide/mcode-hourly",
        ReleaseChannel::Daily => "mcode-ide/mcode-daily",
        ReleaseChannel::Adhoc => "mcode-ide/mcode-adhoc",
    }
}

/// Node spelling for the platform-keyed asset tables
/// (runtime_rpc.rs:473-476 pattern; the original keys on `process.platform`).
fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        "linux" => "linux",
        other => other,
    }
}

/// Mirrors `PLATFORM_UPDATE_MANIFESTS` (orca/src/shared/release-channel.ts:260-265).
fn update_manifest_names_for_platform(platform: &str) -> &'static [&'static str] {
    match platform {
        "darwin" => &["latest-mac.yml"],
        "win32" => &["latest.yml"],
        // Both, because one release carries x64 and arm64 and either makes it installable.
        "linux" => &["latest-linux.yml", "latest-linux-arm64.yml"],
        _ => &[],
    }
}

/// Mirrors `hasInstallableArtifactForPlatform` (release-channel.ts:272-283):
/// permissive on an unknown platform — a filter that hides every build is a
/// worse failure than one that offers a build the download step reports on.
fn has_installable_artifact_for_platform(platform: &str, asset_names: &[String]) -> bool {
    let manifests = update_manifest_names_for_platform(platform);
    if manifests.is_empty() {
        return true;
    }
    manifests
        .iter()
        .any(|manifest| asset_names.iter().any(|name| name == manifest))
}

/// Mirrors `PLATFORM_INSTALLER_PATTERNS` (release-channel.ts:288-292) and
/// `findInstallerAssetName` (:294-303), expressed as case-insensitive
/// suffix tests.
fn find_installer_asset_name(platform: &str, asset_names: &[String]) -> Option<String> {
    let matches = |name: &str| match platform {
        "darwin" => name.to_ascii_lowercase().ends_with(".dmg"),
        "win32" => name.to_ascii_lowercase().ends_with("windows-setup.exe"),
        "linux" => name.to_ascii_lowercase().ends_with(".appimage"),
        _ => false,
    };
    asset_names.iter().find(|name| matches(name)).cloned()
}

/// Mirrors `normalizeTagToVersion` (release-channel.ts:121-123): strips one
/// leading `v`/`V`.
fn normalize_tag_to_version(tag: &str) -> &str {
    tag.strip_prefix(['v', 'V']).unwrap_or(tag)
}

/// A parsed app version: three numeric core components plus prerelease
/// identifiers. Mirrors `parseVersion` (orca/src/shared/app-version.ts:6-19).
struct ParsedVersion {
    core: [u64; 3],
    prerelease: Vec<String>,
}

fn parse_version(value: &str) -> Option<ParsedVersion> {
    let normalized = normalize_tag_to_version(value.trim());
    let (core_part, prerelease_part) = match normalized.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (normalized, None),
    };
    // A build suffix (+…) is accepted but ignored, like the original's regex.
    let core_part = core_part.split('+').next().unwrap_or(core_part);
    let mut core = [0u64; 3];
    let mut segments = core_part.split('.');
    for slot in &mut core {
        let segment = segments.next()?;
        if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *slot = segment.parse().ok()?;
    }
    if segments.next().is_some() {
        return None;
    }
    let prerelease_source = prerelease_part
        .map(|pre| pre.split_once('+').map(|(pre, _)| pre).unwrap_or(pre))
        .unwrap_or_default();
    if prerelease_part.is_some() && prerelease_source.is_empty() {
        return None;
    }
    let prerelease = prerelease_source
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Some(ParsedVersion { core, prerelease })
}

/// Port of `compareAppVersions` (orca/src/shared/app-version.ts:52-96):
/// numeric identifier compare beats lexicographic; a release outranks any
/// prerelease; unparseable inputs compare equal (0).
fn compare_app_versions(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (Some(left), Some(right)) = (parse_version(left), parse_version(right)) else {
        return Ordering::Equal;
    };
    for index in 0..3 {
        if left.core[index] != right.core[index] {
            return left.core[index].cmp(&right.core[index]);
        }
    }
    match (left.prerelease.is_empty(), right.prerelease.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for index in 0..left.prerelease.len().max(right.prerelease.len()) {
                let Some(left_id) = left.prerelease.get(index) else {
                    return Ordering::Less;
                };
                let Some(right_id) = right.prerelease.get(index) else {
                    return Ordering::Greater;
                };
                let left_numeric =
                    !left_id.is_empty() && left_id.bytes().all(|b| b.is_ascii_digit());
                let right_numeric =
                    !right_id.is_empty() && right_id.bytes().all(|b| b.is_ascii_digit());
                let ordering = match (left_numeric, right_numeric) {
                    (true, true) => left_id
                        .parse::<u64>()
                        .unwrap_or(0)
                        .cmp(&right_id.parse::<u64>().unwrap_or(0)),
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => left_id.cmp(right_id),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            Ordering::Equal
        }
    }
}

fn is_hourly_version(version: &str) -> bool {
    // `1.4.160-hourly.202607281400` — release-channel.ts:127.
    let mut parts = version.splitn(2, "-hourly.");
    parts.next().is_some()
        && parts
            .next()
            .is_some_and(|stamp| stamp.len() == 12 && stamp.bytes().all(|b| b.is_ascii_digit()))
}

fn is_daily_version(version: &str) -> bool {
    // `1.4.160-daily.202607281415` — release-channel.ts:132.
    let mut parts = version.splitn(2, "-daily.");
    parts.next().is_some()
        && parts
            .next()
            .is_some_and(|stamp| stamp.len() == 12 && stamp.bytes().all(|b| b.is_ascii_digit()))
}

fn is_adhoc_version(version: &str) -> bool {
    // `1.4.160-adhoc.20260728140533` — release-channel.ts:143.
    let mut parts = version.splitn(2, "-adhoc.");
    parts.next().is_some()
        && parts
            .next()
            .is_some_and(|stamp| stamp.len() == 14 && stamp.bytes().all(|b| b.is_ascii_digit()))
}

/// Port of `getVersionChannel` (orca/src/shared/release-channel.ts:220-237):
/// the dev channels are tested first because they are prereleases too and the
/// `-` catch-all would otherwise file every one of them under rc.
fn version_channel(version: &str) -> Option<&'static str> {
    if is_hourly_version(version) {
        return Some("hourly");
    }
    if is_daily_version(version) {
        return Some("daily");
    }
    if is_adhoc_version(version) {
        return Some("adhoc");
    }
    if version.contains('-') {
        Some("rc")
    } else {
        Some("stable")
    }
}

/// Port of `parseReleaseEntry` (orca/src/main/updater-release-builds.ts:43-84)
/// over a defensive `serde_json::Value` — the original treats every field as
/// `unknown` for the same reason: the GitHub wire format is not trusted.
fn parse_release_entry(
    entry: &serde_json::Value,
    repo: &str,
    platform: &str,
) -> Option<ReleaseBuild> {
    let tag = entry.get("tag_name")?.as_str()?;
    if entry.get("draft").and_then(serde_json::Value::as_bool) == Some(true) {
        return None;
    }
    let version = normalize_tag_to_version(tag).to_owned();
    let channel = version_channel(&version)?;
    parse_version(&version)?;
    // Why filter on assets rather than on a per-channel platform table: a
    // release is published as soon as one platform's leg finishes, and a leg
    // can fail outright (updater-release-builds.ts:57-60).
    let asset_names: Vec<String> = entry
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| asset.get("name").and_then(serde_json::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if !has_installable_artifact_for_platform(platform, &asset_names) {
        return None;
    }
    let installer_asset = find_installer_asset_name(platform, &asset_names);
    // Why null when it merely repeats the tag: GitHub titles an untitled
    // release with its tag name (updater-release-builds.ts:66-69).
    let name = entry
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != tag)
        .map(str::to_owned);
    let published_at = entry
        .get("published_at")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let release_url = entry
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://github.com/{repo}/releases/tag/{tag}"));
    let installer_url = installer_asset
        .map(|asset| format!("https://github.com/{repo}/releases/download/{tag}/{asset}"));
    Some(ReleaseBuild {
        tag: tag.to_owned(),
        version,
        channel: channel.to_owned(),
        name,
        published_at,
        release_url,
        installer_url,
    })
}

/// Port of `listReleaseBuilds` (orca/src/main/updater-release-builds.ts:95-123).
/// No credentials exist compile-time or otherwise — the original also calls the
/// unauthenticated GitHub REST API (`net.fetch` with only an Accept header,
/// updater-release-builds.ts:100-103), so there is no "unconfigured" fallback
/// path to mirror: failures come back as `{ ok: false, message }` data, exactly
/// like the Electron handler (attach-main-window-services.ts:598-604).
fn list_release_builds(channel: ReleaseChannel) -> ReleaseBuildListResult {
    let repo = repo_for_channel(channel);
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page={MAX_LISTED_BUILDS}");
    // One rustls provider per process, matching tauri-plugin-updater's TLS
    // stack (speech_commands.rs:472-475 pattern).
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(FETCH_TIMEOUT_MS))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return builds_error(channel, format!("Could not list {channel} builds: {error}"));
        }
    };
    let response = match client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return builds_error(channel, format!("Could not list {channel} builds: {error}"));
        }
    };
    let status = response.status();
    if !status.is_success() {
        // Error strings copied from updater-release-builds.ts:105-111.
        return builds_error(
            channel,
            match status.as_u16() {
                404 => format!("No releases repository found at {repo}."),
                403 | 429 => "GitHub rate limit reached. Try again in a few minutes.".to_owned(),
                _ => format!(
                    "Could not list {channel} builds (HTTP {}).",
                    status.as_u16()
                ),
            },
        );
    }
    let payload: serde_json::Value = match response.json() {
        Ok(payload) => payload,
        Err(_) => {
            return builds_error(
                channel,
                format!("Could not read the {channel} release list."),
            );
        }
    };
    let Some(entries) = payload.as_array() else {
        return builds_error(
            channel,
            format!("Could not read the {channel} release list."),
        );
    };
    let platform = node_platform();
    let mut builds: Vec<ReleaseBuild> = entries
        .iter()
        .filter_map(|entry| parse_release_entry(entry, repo, platform))
        // Why: the main repo serves both stable and rc, so filter to the
        // asked-for channel (updater-release-builds.ts:120-121).
        .filter(|build| build.channel == channel.to_string())
        .collect();
    // Newest first (sortReleaseBuildsNewestFirst, release-channel.ts:321-323).
    builds.sort_by(|left, right| compare_app_versions(&right.version, &left.version));
    ReleaseBuildListResult::Ok(ReleaseBuildListOk {
        ok: true,
        channel,
        builds,
    })
}

/// Port of the `updater:listBuilds` handler
/// (orca/src/main/window/attach-main-window-services.ts:592-606). An unknown
/// channel cannot reach this command — serde rejects it during invoke
/// deserialization, which is stricter than the original's
/// `{ ok: false, message: 'Unknown release channel …' }` data response but
/// unreachable-in-practice from the typed renderer client.
#[tauri::command]
pub(crate) async fn update_list_builds(
    channel: ReleaseChannel,
) -> Result<ReleaseBuildListResult, DesktopCommandError> {
    // Blocking HTTP off the main thread; the async command wrapper runs on the
    // async runtime (app_dialogs.rs:63-65 discipline).
    tauri::async_runtime::spawn_blocking(move || list_release_builds(channel))
        .await
        .map_err(|error| {
            DesktopCommandError::new(
                "update_list_builds_failed",
                "update_list_builds",
                format!("release list task failed: {error}"),
                "MCode could not list release builds. Try again.",
                None,
                None,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_channel_matches_shared_classification() {
        assert_eq!(version_channel("1.4.160"), Some("stable"));
        assert_eq!(version_channel("1.4.160-rc.1"), Some("rc"));
        assert_eq!(
            version_channel("1.4.160-hourly.202607281400"),
            Some("hourly")
        );
        assert_eq!(version_channel("1.4.160-daily.202607281415"), Some("daily"));
        assert_eq!(
            version_channel("1.4.160-adhoc.20260728140533"),
            Some("adhoc")
        );
    }

    #[test]
    fn version_compare_matches_semver_prerelease_ordering() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_app_versions("1.4.160", "1.4.159"),
            Ordering::Greater
        );
        assert_eq!(
            compare_app_versions("1.4.160", "1.4.160-rc.1"),
            Ordering::Greater
        );
        assert_eq!(
            compare_app_versions("1.4.160-rc.2", "1.4.160-rc.10"),
            Ordering::Less
        );
        assert_eq!(
            compare_app_versions("1.4.160-hourly.202607281400", "1.4.160-hourly.202607281359"),
            Ordering::Greater
        );
        assert_eq!(compare_app_versions("garbage", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn normalize_tag_strips_one_leading_v() {
        assert_eq!(normalize_tag_to_version("v1.4.160"), "1.4.160");
        assert_eq!(normalize_tag_to_version("V1.4.160"), "1.4.160");
        assert_eq!(normalize_tag_to_version("1.4.160"), "1.4.160");
    }

    #[test]
    fn installer_assets_match_platform_patterns() {
        let assets: Vec<String> = ["mcode-1.4.160-windows-setup.exe", "latest.yml"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        assert_eq!(
            find_installer_asset_name("win32", &assets).as_deref(),
            Some("mcode-1.4.160-windows-setup.exe")
        );
        assert_eq!(find_installer_asset_name("darwin", &assets), None);
        assert!(has_installable_artifact_for_platform("win32", &assets));
        assert!(!has_installable_artifact_for_platform("darwin", &assets));
        // Permissive on unknown platforms (release-channel.ts:277-280).
        assert!(has_installable_artifact_for_platform("freebsd", &[]));
    }

    #[test]
    fn parse_release_entry_skips_drafts_and_repeats_tag_title() {
        let entry = serde_json::json!({
            "tag_name": "v1.4.160",
            "name": "v1.4.160",
            "draft": false,
            "published_at": "2026-07-28T14:00:00Z",
            "html_url": "https://github.com/mcode-ide/mcode/releases/tag/v1.4.160",
            "assets": [
                { "name": "latest.yml" },
                { "name": "mcode-1.4.160-windows-setup.exe" }
            ]
        });
        let build = parse_release_entry(&entry, "mcode-ide/mcode", "win32").expect("parses");
        assert_eq!(build.version, "1.4.160");
        assert_eq!(build.channel, "stable");
        // Title repeats the tag → null (updater-release-builds.ts:66-69).
        assert_eq!(build.name, None);
        assert!(
            build
                .installer_url
                .as_deref()
                .unwrap_or("")
                .ends_with("windows-setup.exe")
        );

        let draft = serde_json::json!({ "tag_name": "v1.4.161", "draft": true, "assets": [] });
        assert!(parse_release_entry(&draft, "mcode-ide/mcode", "win32").is_none());
    }
}
