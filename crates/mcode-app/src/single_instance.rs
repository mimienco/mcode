//! Single-instance guard over the OS-level named mutex, mirroring
//! `src/main/startup/single-instance-lock.ts`: packaged macOS bypass env,
//! dev-lane skip unless the E2E enforce env is set, serve-mode duplicate
//! detection (a second `mcode serve` is a supervisor artifact — fail open),
//! and the stable already-running exit code 3 that systemd units key off.

/// single-instance-lock.ts:4 — stable failure message.
pub const SINGLE_INSTANCE_LOCK_FAILURE_MESSAGE: &str = "[single-instance] Another MCode instance is already running for this userData profile; exiting this launch after requesting the existing window. If no MCode process is running, this may be an Electron/macOS single-instance lock failure.";
/// single-instance-lock.ts:5.
pub const SINGLE_INSTANCE_LOCK_BYPASS_ENV: &str = "MCODE_BYPASS_SINGLE_INSTANCE_LOCK";
/// single-instance-lock.ts:7.
pub const SINGLE_INSTANCE_LOCK_E2E_ENFORCE_ENV: &str = "MCODE_E2E_ENFORCE_SINGLE_INSTANCE_LOCK";
/// single-instance-lock.ts:13 — stable exit code keyed off by systemd units.
pub const SINGLE_INSTANCE_ALREADY_RUNNING_EXIT_CODE: i32 = 3;

struct InstanceLock {
    _mutex: PlatformMutex,
    profile: String,
}

static LOCK: std::sync::Mutex<Option<InstanceLock>> = std::sync::Mutex::new(None);

/// single-instance-lock.ts:18-20 — a duplicate `mcode serve` is a supervisor
/// artifact, not a user asking for a window; fail open when argv is
/// unavailable. Matches only the flag form (the CLI redirect rewrites other
/// forms), so a duplicate start does not promote the headless server.
pub fn should_activate_desktop_for_second_instance(argv: &[String]) -> bool {
    !argv_requests_serve_mode(argv)
}

/// serve-mode-argv.ts — the flag form only.
fn argv_requests_serve_mode(argv: &[String]) -> bool {
    argv.iter().any(|arg| arg == "--serve")
}

/// single-instance-lock.ts:51-64 — packaged macOS diagnostics bypass.
pub fn should_bypass_single_instance_lock(
    env: &dyn Fn(&str) -> Option<String>,
    is_dev: bool,
    is_serve_mode: bool,
    platform: &str,
) -> bool {
    matches!(platform, "darwin" | "macos")
        && !is_dev
        && !is_serve_mode
        && env(SINGLE_INSTANCE_LOCK_BYPASS_ENV).as_deref() == Some("1")
}

/// single-instance-lock.ts:67-73 — dev lane skips the lock unless E2E enforces it.
pub fn should_skip_single_instance_lock(
    env: &dyn Fn(&str) -> Option<String>,
    is_dev: bool,
    is_serve_mode: bool,
) -> bool {
    is_dev && !is_serve_mode && env(SINGLE_INSTANCE_LOCK_E2E_ENFORCE_ENV).as_deref() != Some("1")
}

/// Acquire the profile-scoped lock. Returns `Ok(())` when this instance owns
/// the profile, `Err(already running)` otherwise. The mutex is held for the
/// process lifetime via the static — except for a deliberate restart, where
/// `release_for_restart` hands it back so the relaunch child can acquire it.
// `Err(())` is a deliberate boolean-shaped outcome (already running), not a
// missing error type; the caller maps it to the stable exit code 3.
#[allow(clippy::result_unit_err)]
pub fn acquire(profile: &str) -> Result<(), ()> {
    let mut slot = LOCK.lock().map_err(|_| ())?;
    if let Some(lock) = slot.as_ref() {
        return if lock.profile == profile {
            Ok(())
        } else {
            Err(())
        };
    }
    *slot = Some(InstanceLock {
        _mutex: PlatformMutex::acquire(profile)?,
        profile: profile.to_owned(),
    });
    Ok(())
}

/// Deliberate-restart handback: drop the process-lifetime guard so the
/// relaunch child can acquire the same profile mutex. A no-op when the
/// process never acquired (dev skip / macOS bypass).
pub fn release_for_restart() {
    if let Ok(mut slot) = LOCK.lock() {
        *slot = None;
    }
}

/// Platform mutex: named mutex on Windows; a bound TCP listener port is NOT
/// used (the profile is the scope, not a port). POSIX holds a file lock in
/// a private per-user directory until the guard is dropped.
struct PlatformMutex {
    _guard: MutexGuard,
}

enum MutexGuard {
    // The HANDLE is intentionally never read: holding it open is what keeps
    // the mutex acquired for the process lifetime (dropping would close it).
    #[cfg(windows)]
    #[allow(dead_code)]
    Windows(windows_sys::Win32::Foundation::HANDLE),
    #[cfg(unix)]
    Posix(std::fs::File),
}

// SAFETY: the wrapped HANDLE is a process-wide kernel object handle. It is
// only closed once by its owning guard, and the kernel object supports
// process-wide handle lifetime. Raw pointers are not automatically Send/Sync, so state it.
#[cfg(windows)]
unsafe impl Send for MutexGuard {}
#[cfg(windows)]
unsafe impl Sync for MutexGuard {}

fn profile_digest(profile: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(profile.as_bytes()))
}

impl Drop for MutexGuard {
    fn drop(&mut self) {
        match self {
            #[cfg(windows)]
            Self::Windows(handle) => unsafe {
                windows_sys::Win32::Foundation::CloseHandle(*handle);
            },
            #[cfg(unix)]
            Self::Posix(file) => {
                let _ = file.unlock();
            }
        }
    }
}

impl PlatformMutex {
    fn acquire(profile: &str) -> Result<Self, ()> {
        #[cfg(windows)]
        {
            // Kernel mutex names treat "\" as the Global\Local prefix
            // separator; arbitrary backslashes make CreateMutexW fail with
            // ERROR_PATH_NOT_FOUND (verified by probe). Flatten the profile
            // into one path-free segment.
            let name: Vec<u16> = format!("mcode-single-instance-{}", profile_digest(profile))
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            // Canonical duplicate detection: CreateMutexW with
            // CREATE_INITIAL_OWNER succeeds even when the mutex exists, and
            // GetLastError() then reports ERROR_ALREADY_EXISTS (183) — that
            // is the "another instance owns the profile" signal (the
            // creator is the initial owner otherwise).
            const ERROR_ALREADY_EXISTS: u32 = 183;
            let exists: i32 =
                windows_sys::Win32::System::Threading::CREATE_MUTEX_INITIAL_OWNER as i32;
            let handle = unsafe {
                windows_sys::Win32::System::Threading::CreateMutexW(
                    std::ptr::null(),
                    exists,
                    name.as_ptr(),
                )
            };
            if handle.is_null() {
                return Err(());
            }
            let last_error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if last_error == ERROR_ALREADY_EXISTS {
                // Another instance holds the profile: release our probe
                // handle and report already-running.
                unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
                return Err(());
            }
            let _ = last_error;
            let _ = exists;
            Ok(Self {
                _guard: MutexGuard::Windows(handle),
            })
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
            let uid = unsafe { libc::geteuid() };
            let directory = std::env::temp_dir().join(format!("mcode-single-instance-{uid}"));
            match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(()),
            }
            let metadata = std::fs::symlink_metadata(&directory).map_err(|_| ())?;
            if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
                return Err(());
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(directory.join(format!("{}.lock", profile_digest(profile))))
                .map_err(|_| ())?;
            let metadata = file.metadata().map_err(|_| ())?;
            if !metadata.is_file() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
                return Err(());
            }
            file.try_lock().map_err(|_| ())?;
            // Leave the inode in place: unlinking a held/released lock permits a competing lock inode.
            Ok(Self {
                _guard: MutexGuard::Posix(file),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(map: &[(String, String)]) -> impl Fn(&str) -> Option<String> + '_ {
        move |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn serve_mode_argv_does_not_activate_desktop() {
        // The systemd unit runs `<binary> serve --port …`; a duplicate must
        // not promote the live headless server (#11935).
        assert!(!should_activate_desktop_for_second_instance(&[
            "mcode".into(),
            "serve".into(),
            "--serve".into(),
        ]));
        assert!(should_activate_desktop_for_second_instance(&[
            "mcode".into()
        ]));
    }

    #[test]
    fn bypass_is_macos_packaged_only() {
        let env_pairs = [(SINGLE_INSTANCE_LOCK_BYPASS_ENV.to_string(), "1".to_string())];
        let env = env_of(&env_pairs);
        assert!(should_bypass_single_instance_lock(
            &env, false, false, "darwin"
        ));
        assert!(!should_bypass_single_instance_lock(
            &env, true, false, "darwin"
        ));
        assert!(!should_bypass_single_instance_lock(
            &env, false, true, "darwin"
        ));
        assert!(!should_bypass_single_instance_lock(
            &env, false, false, "win32"
        ));
    }

    #[test]
    fn dev_lane_skips_unless_e2e_enforces() {
        let no_env = env_of(&[]);
        let enforce_pairs = [(
            SINGLE_INSTANCE_LOCK_E2E_ENFORCE_ENV.to_string(),
            "1".to_string(),
        )];
        let enforced = env_of(&enforce_pairs);
        assert!(should_skip_single_instance_lock(&no_env, true, false));
        assert!(!should_skip_single_instance_lock(&enforced, true, false));
        assert!(!should_skip_single_instance_lock(&no_env, false, false));
        assert!(!should_skip_single_instance_lock(&no_env, true, true));
    }

    #[test]
    fn second_acquire_fails_for_the_same_profile() {
        // Exercises the OS mutex directly (not the OnceLock-guarded
        // `acquire`, whose static makes cross-test isolation impossible):
        // two live handles for one profile must observe each other, since
        // WaitForSingleObject with zero timeout reports WAIT_TIMEOUT while
        // the first guard still owns the kernel mutex.
        let _first = PlatformMutex::acquire("single-instance-e2e-probe")
            .expect("first acquire owns the profile");
        assert!(
            PlatformMutex::acquire("single-instance-e2e-probe").is_err(),
            "second acquire for the same profile must report already-running"
        );
    }
    #[test]
    fn releasing_profile_guard_allows_reacquisition_and_other_profiles_remain_independent() {
        let profile = format!("lock-release-{}", std::process::id());
        let first = PlatformMutex::acquire(&profile).unwrap();
        assert!(PlatformMutex::acquire(&profile).is_err());
        let other = PlatformMutex::acquire(&format!("{profile}-other")).unwrap();
        drop(first);
        let replacement = PlatformMutex::acquire(&profile).unwrap();
        drop(replacement);
        drop(other);
    }

    #[test]
    fn profile_paths_are_encoded_without_kernel_name_separators() {
        let digest = profile_digest(r"C:\Users\Example\MCode");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(digest, profile_digest(r"C:\Users\Other\MCode"));
    }
    #[test]
    #[ignore = "subprocess helper invoked by profile_lock_is_visible_to_another_process"]
    fn profile_lock_child_probe() {
        let profile = std::env::var("MCODE_SINGLE_INSTANCE_TEST_PROFILE").unwrap();
        let expected = std::env::var("MCODE_SINGLE_INSTANCE_TEST_ACQUIRED").unwrap() == "1";
        assert_eq!(PlatformMutex::acquire(&profile).is_ok(), expected);
    }

    #[test]
    fn profile_lock_is_visible_to_another_process() {
        let profile = format!("process-lock-{}", std::process::id());
        let lock = PlatformMutex::acquire(&profile).unwrap();
        let probe = |expected: bool| {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "single_instance::tests::profile_lock_child_probe",
                    "--ignored",
                ])
                .env("MCODE_SINGLE_INSTANCE_TEST_PROFILE", &profile)
                .env(
                    "MCODE_SINGLE_INSTANCE_TEST_ACQUIRED",
                    if expected { "1" } else { "0" },
                );
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        };
        probe(false);
        drop(lock);
        probe(true);
    }
}
