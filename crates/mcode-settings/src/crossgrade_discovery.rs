//! Crossgrade discovery: find the last Electron installation's userData dir
//! without touching it. Migration is read-then-copy; the Electron location
//! stays intact until a verified migration completes (see
//! `docs/migration/DATA_AND_SETTINGS_MIGRATION.md` invariants).

use std::path::PathBuf;

/// Packaged Electron identity (electron-builder `productName: MCode`, app
/// name `mcode`). On Windows Electron resolves userData to
/// `%APPDATA%\<appName>`; on macOS `~/Library/Application Support/<appName>`;
/// on XDG Linux `$XDG_CONFIG_HOME/<appName>` (defaulting to
/// `~/.config/<appName>`).
pub const ELECTRON_PROD_APP_NAME: &str = "MCode";

/// Stable marker files that identify a real MCode userData directory. A
/// directory that lacks all of them is not an MCode profile (avoids adopting
/// an unrelated app's folder that happens to share the name).
const MARKER_FILES: [&str; 4] = [
    "mcode-data.json",
    "mcode-profile-index.json",
    "mcode-e2ee-keypair.json",
    "agent-hooks",
];

/// A discovered, non-destructive crossgrade candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElectronDataLocation {
    pub path: PathBuf,
    /// Marker files that were present — progress-report material only.
    pub markers: Vec<&'static str>,
}

impl ElectronDataLocation {
    /// True when the location holds the primary settings document.
    pub fn has_primary_settings(&self) -> bool {
        self.path.join("mcode-data.json").is_file()
    }
}

fn resolve_base_dir(
    env_var: &str,
    fallback: &str,
    app_name: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    let base = env(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(fallback));
    if base.as_os_str().is_empty() {
        return None;
    }
    Some(base.join(app_name))
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Enumerate candidate Electron userData dirs, best-first. Windows prod
/// (`%APPDATA%/MCode`) precedes the XDG fallback; dev identities
/// (`MCode Dev`, branch labels) are intentionally NOT adopted — a dev
/// instance's data belongs to its disposable lane.
pub fn discover_electron_data_locations() -> Vec<ElectronDataLocation> {
    discover_electron_data_locations_with(&|key| std::env::var(key).ok())
}

/// Injectable variant for tests and diagnostics.
fn discover_electron_data_locations_with(
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<ElectronDataLocation> {
    let mut candidates = Vec::new();
    for (env_var, fallback) in [
        ("APPDATA", "AppData/Roaming"),
        ("XDG_CONFIG_HOME", ".config"),
    ] {
        if let Some(dir) = resolve_base_dir(env_var, fallback, ELECTRON_PROD_APP_NAME, env) {
            candidates.push(dir);
        }
    }
    candidates
        .into_iter()
        .filter_map(|path| {
            let markers: Vec<&'static str> = MARKER_FILES
                .iter()
                .copied()
                .filter(|marker| path.join(marker).exists())
                .collect();
            if markers.is_empty() {
                None
            } else {
                Some(ElectronDataLocation { path, markers })
            }
        })
        .collect()
}

/// The single best crossgrade candidate: the first discovered location whose
/// primary settings document exists. Read-only; never creates or modifies.
pub fn find_electron_settings_source() -> Option<PathBuf> {
    find_electron_settings_source_with(&|key| std::env::var(key).ok())
}

fn find_electron_settings_source_with(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    discover_electron_data_locations_with(env)
        .into_iter()
        .find(|location| location.has_primary_settings())
        .map(|location| location.path.join("mcode-data.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(base: &std::path::Path) -> impl Fn(&str) -> Option<String> + '_ {
        move |key: &str| {
            if key == "APPDATA" {
                Some(base.to_string_lossy().to_string())
            } else {
                None
            }
        }
    }

    #[test]
    fn marker_files_distinguish_real_profiles() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(ELECTRON_PROD_APP_NAME)).unwrap();
        let env = env_of(dir.path());
        assert!(find_electron_settings_source_with(&env).is_none());
    }

    #[test]
    fn finds_settings_when_primary_document_exists() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join(ELECTRON_PROD_APP_NAME);
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("mcode-data.json"), "{}").unwrap();
        let env = env_of(dir.path());
        assert_eq!(
            find_electron_settings_source_with(&env),
            Some(profile.join("mcode-data.json"))
        );
    }

    #[test]
    fn primary_settings_require_a_file_not_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join(ELECTRON_PROD_APP_NAME);
        std::fs::create_dir_all(profile.join("mcode-data.json")).unwrap();
        let env = env_of(dir.path());
        assert!(find_electron_settings_source_with(&env).is_none());
    }

    #[test]
    fn directory_with_only_foreign_files_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join(ELECTRON_PROD_APP_NAME);
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("unrelated.json"), "{}").unwrap();
        let env = env_of(dir.path());
        assert!(find_electron_settings_source_with(&env).is_none());
    }
}
