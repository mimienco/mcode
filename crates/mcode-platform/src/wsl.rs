use std::path::PathBuf;

#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::sync::{Mutex, OnceLock};
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
static WSL_HOME_CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

/// Returns the distro user's actual `$HOME` as a Windows WSL UNC path.
/// Successful probes are cached; failures remain retryable for stopped distros.
pub fn wsl_home_directory(distro: &str) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        let _ = distro;
        None
    }
    #[cfg(windows)]
    {
        if !valid_wsl_distro_name(distro) {
            return None;
        }
        let cache = WSL_HOME_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(home) = cache.lock().ok()?.get(distro).cloned() {
            return Some(home);
        }
        let home = probe_wsl_home(distro)?;
        cache.lock().ok()?.insert(distro.to_owned(), home.clone());
        Some(home)
    }
}

#[cfg(windows)]
fn valid_wsl_distro_name(distro: &str) -> bool {
    !distro.trim().is_empty()
        && !distro
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
}

#[cfg(windows)]
fn probe_wsl_home(distro: &str) -> Option<PathBuf> {
    let mut child = Command::new("wsl.exe")
        .args(["-d", distro, "--exec", "bash", "-c", "printf %s \"$HOME\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().ok()?;
                if !output.status.success() {
                    return None;
                }
                let home = String::from_utf8(output.stdout).ok()?;
                return wsl_home_unc_path(distro, home.trim());
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(any(windows, test))]
fn wsl_home_unc_path(distro: &str, home: &str) -> Option<PathBuf> {
    let segments = home
        .strip_prefix('/')?
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| matches!(*segment, "." | "..") || segment.contains('\\'))
    {
        return None;
    }
    Some(PathBuf::from(format!(
        r"\\wsl.localhost\{distro}\{}",
        segments.join(r"\")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_only_absolute_normalized_wsl_home_paths() {
        assert_eq!(
            wsl_home_unc_path("Ubuntu", "/custom/alice"),
            Some(PathBuf::from(r"\\wsl.localhost\Ubuntu\custom\alice"))
        );
        assert_eq!(wsl_home_unc_path("Ubuntu", "relative/home"), None);
        assert_eq!(wsl_home_unc_path("Ubuntu", "/home/../root"), None);
    }
}
