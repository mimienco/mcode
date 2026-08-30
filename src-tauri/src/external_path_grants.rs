use mcode_fs::{ExactPathCapability, FsError};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const DEFAULT_MAX_GRANTS_PER_WINDOW: usize = 4096;

#[derive(Debug, Default)]
struct WindowGrants {
    generation: u64,
    pending_native_paths: VecDeque<PathBuf>,
    active: VecDeque<ExactPathCapability>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalPathGrantRegistry {
    inner: Arc<Mutex<HashMap<String, WindowGrants>>>,
    max_grants_per_window: usize,
}

impl Default for ExternalPathGrantRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_GRANTS_PER_WINDOW)
    }
}

impl ExternalPathGrantRegistry {
    #[cfg(test)]
    fn with_limit(max_grants_per_window: usize) -> Self {
        Self::new(max_grants_per_window)
    }

    fn new(max_grants_per_window: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_grants_per_window,
        }
    }

    pub(crate) fn stage_native_paths(
        &self,
        window_label: &str,
        paths: &[PathBuf],
    ) -> Result<(), FsError> {
        let mut grants = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let window_grants = grants.entry(window_label.to_owned()).or_default();
        for path in paths {
            if let Some(index) = window_grants
                .pending_native_paths
                .iter()
                .position(|pending| pending == path)
            {
                window_grants.pending_native_paths.remove(index);
            }
            window_grants.pending_native_paths.push_back(path.clone());
        }
        while window_grants.pending_native_paths.len() > self.max_grants_per_window {
            window_grants.pending_native_paths.pop_front();
        }
        Ok(())
    }

    pub(crate) fn claim(&self, window_label: &str, path: &Path) -> Result<bool, FsError> {
        self.claim_with(window_label, path, |target| {
            ExactPathCapability::authorize(target)
        })
    }

    fn claim_with(
        &self,
        window_label: &str,
        path: &Path,
        authorize: impl FnOnce(&Path) -> Result<ExactPathCapability, FsError>,
    ) -> Result<bool, FsError> {
        let generation = {
            let mut grants = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
            let Some(window_grants) = grants.get_mut(window_label) else {
                return Ok(false);
            };
            if let Some(index) = window_grants
                .active
                .iter()
                .position(|capability| capability.matches(path))
            {
                if let Some(pending_index) = window_grants
                    .pending_native_paths
                    .iter()
                    .position(|pending| pending == path)
                {
                    window_grants.pending_native_paths.remove(pending_index);
                }
                let capability = window_grants
                    .active
                    .remove(index)
                    .expect("matched active capability must exist");
                window_grants.active.push_back(capability);
                return Ok(true);
            }
            let Some(index) = window_grants
                .pending_native_paths
                .iter()
                .position(|pending| pending == path)
            else {
                return Ok(false);
            };
            window_grants.pending_native_paths.remove(index);
            window_grants.generation
        };

        let capability = authorize(path)?;

        let mut grants = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let window_grants = grants.entry(window_label.to_owned()).or_default();
        if window_grants.generation != generation {
            return Ok(false);
        }
        if let Some(index) = window_grants.active.iter().position(|existing| {
            existing.matches(path) || existing.matches(capability.canonical_path())
        }) {
            window_grants.active.remove(index);
        }
        window_grants.active.push_back(capability);
        while window_grants.active.len() > self.max_grants_per_window {
            window_grants.active.pop_front();
        }
        Ok(true)
    }

    pub(crate) fn capability_for(
        &self,
        window_label: &str,
        path: &Path,
    ) -> Result<Option<ExactPathCapability>, FsError> {
        let mut grants = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let Some(window_grants) = grants.get_mut(window_label) else {
            return Ok(None);
        };
        let Some(index) = window_grants
            .active
            .iter()
            .position(|capability| capability.matches(path))
        else {
            return Ok(None);
        };
        let capability = window_grants
            .active
            .remove(index)
            .expect("matched active capability must exist");
        window_grants.active.push_back(capability.clone());
        Ok(Some(capability))
    }

    pub(crate) fn revoke(&self, window_label: &str, path: &Path) -> Result<bool, FsError> {
        let mut grants = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let Some(window_grants) = grants.get_mut(window_label) else {
            return Ok(false);
        };
        let pending_revoked = window_grants
            .pending_native_paths
            .iter()
            .position(|pending| pending == path)
            .and_then(|index| window_grants.pending_native_paths.remove(index))
            .is_some();
        let active_revoked = window_grants
            .active
            .iter()
            .position(|capability| capability.matches(path))
            .and_then(|index| window_grants.active.remove(index))
            .is_some();
        Ok(pending_revoked || active_revoked)
    }

    pub(crate) fn revoke_window(&self, window_label: &str) -> Result<usize, FsError> {
        let mut grants = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let window_grants = grants.entry(window_label.to_owned()).or_default();
        window_grants.generation = window_grants
            .generation
            .checked_add(1)
            .ok_or(FsError::StateUnavailable)?;
        window_grants.pending_native_paths.clear();
        let revoked = window_grants.active.len();
        window_grants.active.clear();
        Ok(revoked)
    }
}

#[cfg(test)]
mod tests {
    use super::ExternalPathGrantRegistry;
    use mcode_fs::ExactPathCapability;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mcode-external-grants-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn stage_and_claim(registry: &ExternalPathGrantRegistry, window_label: &str, path: &Path) {
        registry
            .stage_native_paths(window_label, &[path.to_owned()])
            .unwrap();
        assert!(registry.claim(window_label, path).unwrap());
    }

    #[test]
    fn claims_require_a_native_path_staged_for_the_same_window() {
        let root = TestDirectory::new();
        let staged = root.path().join("staged.txt");
        let unstaged = root.path().join("unstaged.txt");
        fs::write(&staged, "staged").unwrap();
        fs::write(&unstaged, "unstaged").unwrap();
        let registry = ExternalPathGrantRegistry::default();
        registry
            .stage_native_paths("window-a", std::slice::from_ref(&staged))
            .unwrap();

        assert!(!registry.claim("window-b", &staged).unwrap());
        assert!(!registry.claim("window-a", &unstaged).unwrap());
        assert!(registry.claim("window-a", &staged).unwrap());
        assert!(registry.claim("window-a", &staged).unwrap());
        registry
            .stage_native_paths("window-a", std::slice::from_ref(&staged))
            .unwrap();
        assert!(registry.revoke("window-a", &staged).unwrap());
        assert!(!registry.claim("window-a", &staged).unwrap());
    }

    #[test]
    fn grants_are_window_scoped_and_revocation_is_idempotent() {
        let root = TestDirectory::new();
        let target = root.path().join("external.txt");
        fs::write(&target, "external").unwrap();
        let registry = ExternalPathGrantRegistry::default();

        stage_and_claim(&registry, "window-a", &target);
        assert!(
            registry
                .capability_for("window-a", &target)
                .unwrap()
                .is_some()
        );
        assert!(
            registry
                .capability_for("window-b", &target)
                .unwrap()
                .is_none()
        );

        stage_and_claim(&registry, "window-b", &target);
        assert!(registry.revoke("window-a", &target).unwrap());
        assert!(!registry.revoke("window-a", &target).unwrap());
        assert!(
            registry
                .capability_for("window-b", &target)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn grants_use_a_bounded_per_window_lru() {
        let root = TestDirectory::new();
        let paths = ["a.txt", "b.txt", "c.txt"].map(|name| root.path().join(name));
        for path in &paths {
            fs::write(path, "value").unwrap();
        }
        let registry = ExternalPathGrantRegistry::with_limit(2);

        stage_and_claim(&registry, "main", &paths[0]);
        stage_and_claim(&registry, "main", &paths[1]);
        assert!(registry.claim("main", &paths[0]).unwrap());
        stage_and_claim(&registry, "main", &paths[2]);

        assert!(
            registry
                .capability_for("main", &paths[0])
                .unwrap()
                .is_some()
        );
        assert!(
            registry
                .capability_for("main", &paths[1])
                .unwrap()
                .is_none()
        );
        assert!(
            registry
                .capability_for("main", &paths[2])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn revoking_a_window_invalidates_pending_and_active_grants_only_for_that_window() {
        let root = TestDirectory::new();
        let active = root.path().join("active.txt");
        let pending = root.path().join("pending.txt");
        fs::write(&active, "active").unwrap();
        fs::write(&pending, "pending").unwrap();
        let registry = ExternalPathGrantRegistry::default();
        stage_and_claim(&registry, "window-a", &active);
        stage_and_claim(&registry, "window-b", &active);
        registry
            .stage_native_paths("window-a", std::slice::from_ref(&pending))
            .unwrap();

        assert_eq!(registry.revoke_window("window-a").unwrap(), 1);
        assert!(!registry.claim("window-a", &pending).unwrap());
        assert!(
            registry
                .capability_for("window-a", &active)
                .unwrap()
                .is_none()
        );
        assert!(
            registry
                .capability_for("window-b", &active)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn stale_in_flight_claim_cannot_insert_after_window_invalidation() {
        let root = TestDirectory::new();
        let target = root.path().join("external.txt");
        fs::write(&target, "external").unwrap();
        let registry = ExternalPathGrantRegistry::default();
        registry
            .stage_native_paths("main", std::slice::from_ref(&target))
            .unwrap();
        let authorization_started = Arc::new(Barrier::new(2));
        let continue_authorization = Arc::new(Barrier::new(2));
        let claim_registry = registry.clone();
        let claim_target = target.clone();
        let claim_started = authorization_started.clone();
        let claim_continue = continue_authorization.clone();

        let claim = std::thread::spawn(move || {
            claim_registry.claim_with("main", &claim_target, |path| {
                claim_started.wait();
                claim_continue.wait();
                ExactPathCapability::authorize(path)
            })
        });
        authorization_started.wait();
        assert_eq!(registry.revoke_window("main").unwrap(), 0);
        continue_authorization.wait();

        assert!(!claim.join().unwrap().unwrap());
        assert!(registry.capability_for("main", &target).unwrap().is_none());
    }
}
