use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::git_binary::GitError;

const RETRY_AFTER: Duration = Duration::from_secs(300);

/// Per execution runner, so rejecting one host's binary never disables another host.
#[derive(Default)]
pub(crate) struct GitCapabilityCache {
    worktree_repair_rejected_at: Mutex<Option<Instant>>,
}

impl GitCapabilityCache {
    pub(crate) fn worktree_repair(
        &self,
        version: &str,
        probe: impl FnOnce() -> Result<(), GitError>,
    ) -> Result<(), GitError> {
        self.repair_at(version, Instant::now(), RETRY_AFTER, probe)
    }

    fn repair_at(
        &self,
        version: &str,
        now: Instant,
        retry_after: Duration,
        probe: impl FnOnce() -> Result<(), GitError>,
    ) -> Result<(), GitError> {
        // Hold through the first probe so concurrent callers share its unsupported result.
        let mut rejected_at = self
            .worktree_repair_rejected_at
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let unsupported = || GitError::GitUnsupportedCapability {
            capability: "git worktree repair",
            version: version.to_owned(),
        };
        if rejected_at.is_some_and(|last| now.saturating_duration_since(last) < retry_after) {
            return Err(unsupported());
        }
        match probe() {
            Err(error) if repair_is_unsupported(&error) => {
                *rejected_at = Some(now);
                Err(unsupported())
            }
            result => {
                *rejected_at = None;
                result
            }
        }
    }
}

fn repair_is_unsupported(error: &GitError) -> bool {
    let GitError::GitOperationFailed {
        stderr_redacted, ..
    } = error
    else {
        return false;
    };
    stderr_redacted.lines().any(|line| {
        line.trim()
            .strip_prefix("error: unknown subcommand:")
            .is_some_and(|name| name.trim().trim_matches(['\'', '"', '`']) == "repair")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn rejected() -> GitError {
        GitError::GitOperationFailed {
            command: "worktree repair".into(),
            stderr_redacted: "error: unknown subcommand: `repair'\nusage: git worktree ...".into(),
        }
    }

    #[test]
    fn rejects_once_then_retries_after_upgrade_interval() {
        let cache = GitCapabilityCache::default();
        let now = Instant::now();
        assert!(matches!(
            cache.repair_at("2.25", now, RETRY_AFTER, || Err(rejected())),
            Err(GitError::GitUnsupportedCapability { .. })
        ));
        assert!(
            cache
                .repair_at("2.25", now, RETRY_AFTER, || panic!(
                    "cached rejection should skip subprocess"
                ))
                .is_err()
        );
        assert!(
            cache
                .repair_at("2.25", now + RETRY_AFTER, RETRY_AFTER, || Ok(()))
                .is_ok()
        );
    }

    #[test]
    fn coalesces_concurrent_unsupported_probes() {
        let cache = GitCapabilityCache::default();
        let calls = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    assert!(
                        cache
                            .worktree_repair("2.25", || {
                                calls.fetch_add(1, Ordering::SeqCst);
                                Err(rejected())
                            })
                            .is_err()
                    );
                });
            }
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execution_runners_are_isolated() {
        let native = GitCapabilityCache::default();
        let other_host = GitCapabilityCache::default();
        assert!(native.worktree_repair("2.25", || Err(rejected())).is_err());
        assert!(other_host.worktree_repair("2.25-vendor", || Ok(())).is_ok());
    }

    #[test]
    fn ordinary_failures_do_not_poison_capability() {
        let cache = GitCapabilityCache::default();
        for error in [
            GitError::GitTimeout,
            GitError::GitLockExists,
            GitError::GitOperationFailed {
                command: "worktree repair".into(),
                stderr_redacted: "error: unknown subcommand: other\nrepair failed".into(),
            },
        ] {
            assert!(!repair_is_unsupported(&error));
            assert!(cache.worktree_repair("2.49", || Err(error)).is_err());
            assert!(cache.worktree_repair("2.49", || Ok(())).is_ok());
        }
    }
}
