use mcode_fs::{ExactPathCapability, FsError};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// This is a process-wide bound, not a per-window allowance. File capabilities
// share one pinned OS handle across clones, so bounding retained capabilities
// bounds the registry's native handle ownership as well.
const DEFAULT_MAX_RETAINED_GRANTS: usize = 256;
const MAX_NATIVE_DROP_PATHS: usize = 256;

#[derive(Debug)]
struct RetainedGrant {
    capability: ExactPathCapability,
    last_used: u64,
}

#[derive(Debug)]
struct WindowGrants {
    // Generations are allocated process-wide and never reused. Removing a
    // destroyed window therefore cannot leave an unbounded tombstone, while a
    // later window with the same label receives a distinct identity.
    generation: u64,
    pending_native_grants: VecDeque<RetainedGrant>,
    active: VecDeque<RetainedGrant>,
}

#[derive(Debug)]
struct RegistryState {
    windows: HashMap<String, WindowGrants>,
    next_generation: u64,
    next_usage_sequence: u64,
    capture_reservations: usize,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            windows: HashMap::new(),
            next_generation: 1,
            next_usage_sequence: 1,
            capture_reservations: 0,
        }
    }
}

impl RegistryState {
    fn next_generation(&mut self) -> Result<u64, FsError> {
        let generation = self.next_generation;
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(FsError::StateUnavailable)?;
        Ok(generation)
    }

    fn next_usage_sequence(&mut self) -> Result<u64, FsError> {
        let sequence = self.next_usage_sequence;
        self.next_usage_sequence = self
            .next_usage_sequence
            .checked_add(1)
            .ok_or(FsError::StateUnavailable)?;
        Ok(sequence)
    }

    fn ensure_window(&mut self, window_label: &str) -> Result<u64, FsError> {
        if let Some(window) = self.windows.get(window_label) {
            return Ok(window.generation);
        }
        let generation = self.next_generation()?;
        self.windows.insert(
            window_label.to_owned(),
            WindowGrants {
                generation,
                pending_native_grants: VecDeque::new(),
                active: VecDeque::new(),
            },
        );
        Ok(generation)
    }

    fn retained_grant_count(&self) -> usize {
        self.windows
            .values()
            .map(|window| window.pending_native_grants.len() + window.active.len())
            .sum()
    }

    fn evict_oldest_until_bounded(&mut self, limit: usize) {
        while self.retained_grant_count() > limit {
            let oldest = self
                .windows
                .iter()
                .flat_map(|(label, window)| {
                    window
                        .pending_native_grants
                        .iter()
                        .enumerate()
                        .map(move |(index, grant)| (grant.last_used, label.clone(), true, index))
                        .chain(window.active.iter().enumerate().map(move |(index, grant)| {
                            (grant.last_used, label.clone(), false, index)
                        }))
                })
                .min_by_key(|candidate| candidate.0);
            let Some((_, label, pending, index)) = oldest else {
                break;
            };
            let window = self
                .windows
                .get_mut(&label)
                .expect("an eviction candidate must belong to a live window");
            if pending {
                window.pending_native_grants.remove(index);
            } else {
                window.active.remove(index);
            }
        }
    }

    fn reserve_capture_slot(&mut self, limit: usize) -> Result<(), FsError> {
        while self.retained_grant_count() + self.capture_reservations >= limit {
            let retained_before = self.retained_grant_count();
            self.evict_oldest_until_bounded(retained_before.saturating_sub(1));
            if self.retained_grant_count() == retained_before {
                return Err(FsError::StateUnavailable);
            }
        }
        self.capture_reservations = self
            .capture_reservations
            .checked_add(1)
            .ok_or(FsError::StateUnavailable)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalPathGrantRegistry {
    inner: Arc<Mutex<RegistryState>>,
    max_retained_grants: usize,
}

impl Default for ExternalPathGrantRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_RETAINED_GRANTS)
    }
}

impl ExternalPathGrantRegistry {
    #[cfg(test)]
    fn with_limit(max_retained_grants: usize) -> Self {
        Self::new(max_retained_grants)
    }

    fn new(max_retained_grants: usize) -> Self {
        assert!(max_retained_grants > 0, "grant budget must be non-zero");
        Self {
            inner: Arc::new(Mutex::new(RegistryState::default())),
            max_retained_grants,
        }
    }

    /// Captures every dropped object synchronously at the native event boundary.
    ///
    /// Deferring this open until the renderer claims a raw path would let a local
    /// process replace that path between drop and claim. The event callback pays
    /// only this unavoidable capture cost; renderer claims perform no filesystem
    /// resolution or open and only move an already-captured capability.
    pub(crate) fn stage_native_paths(
        &self,
        window_label: &str,
        paths: &[PathBuf],
    ) -> Result<(), FsError> {
        self.stage_native_paths_with(window_label, paths, |path| {
            ExactPathCapability::authorize(path)
        })
    }

    fn stage_native_paths_with(
        &self,
        window_label: &str,
        paths: &[PathBuf],
        capture: impl Fn(&Path) -> Result<ExactPathCapability, FsError>,
    ) -> Result<(), FsError> {
        if paths.len() > MAX_NATIVE_DROP_PATHS {
            return Err(FsError::ExternalTooManyPaths {
                count: paths.len(),
                limit: MAX_NATIVE_DROP_PATHS,
            });
        }
        let generation = self
            .inner
            .lock()
            .map_err(|_| FsError::StateUnavailable)?
            .ensure_window(window_label)?;

        for path in paths {
            {
                let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
                if state
                    .windows
                    .get(window_label)
                    .map(|window| window.generation)
                    != Some(generation)
                {
                    return Ok(());
                }
                state.reserve_capture_slot(self.max_retained_grants)?;
            }

            let captured = capture(path);
            let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
            state.capture_reservations = state
                .capture_reservations
                .checked_sub(1)
                .ok_or(FsError::StateUnavailable)?;
            let capability = captured?;
            if state
                .windows
                .get(window_label)
                .map(|window| window.generation)
                != Some(generation)
            {
                return Ok(());
            }
            let sequence = state.next_usage_sequence()?;
            let window = state
                .windows
                .get_mut(window_label)
                .expect("the checked window generation must still exist");
            if let Some(index) = window
                .pending_native_grants
                .iter()
                .position(|pending| pending.capability.matches(path))
            {
                window.pending_native_grants.remove(index);
            }
            window.pending_native_grants.push_back(RetainedGrant {
                capability,
                last_used: sequence,
            });
        }
        Ok(())
    }

    pub(crate) fn claim(&self, window_label: &str, path: &Path) -> Result<bool, FsError> {
        let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let Some(window) = state.windows.get(window_label) else {
            return Ok(false);
        };
        let pending_index = window
            .pending_native_grants
            .iter()
            .position(|pending| pending.capability.matches(path));
        let active_index = pending_index.is_none().then(|| {
            window
                .active
                .iter()
                .position(|active| active.capability.matches(path))
        });
        if pending_index.is_none() && active_index.flatten().is_none() {
            return Ok(false);
        }

        let sequence = state.next_usage_sequence()?;
        let window = state
            .windows
            .get_mut(window_label)
            .expect("the matched window must still exist");
        if let Some(index) = pending_index {
            let mut captured = window
                .pending_native_grants
                .remove(index)
                .expect("the matched pending capability must exist");
            window.active.retain(|existing| {
                !existing.capability.matches_root(path)
                    && !existing
                        .capability
                        .matches_root(captured.capability.canonical_path())
            });
            captured.last_used = sequence;
            window.active.push_back(captured);
        } else if let Some(index) = active_index.flatten() {
            let mut active = window
                .active
                .remove(index)
                .expect("the matched active capability must exist");
            active.last_used = sequence;
            window.active.push_back(active);
        }
        state.evict_oldest_until_bounded(self.max_retained_grants);
        Ok(true)
    }

    pub(crate) fn capability_for(
        &self,
        window_label: &str,
        path: &Path,
    ) -> Result<Option<ExactPathCapability>, FsError> {
        let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let Some(index) = state.windows.get(window_label).and_then(|window| {
            window
                .active
                .iter()
                .position(|capability| capability.capability.matches(path))
        }) else {
            return Ok(None);
        };
        let sequence = state.next_usage_sequence()?;
        let window = state
            .windows
            .get_mut(window_label)
            .expect("the matched window must still exist");
        let mut retained = window
            .active
            .remove(index)
            .expect("the matched active capability must exist");
        retained.last_used = sequence;
        let capability = retained.capability.clone();
        window.active.push_back(retained);
        Ok(Some(capability))
    }

    pub(crate) fn revoke(&self, window_label: &str, path: &Path) -> Result<bool, FsError> {
        let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let Some(window) = state.windows.get_mut(window_label) else {
            return Ok(false);
        };
        let before = window.pending_native_grants.len() + window.active.len();
        window
            .pending_native_grants
            .retain(|pending| !pending.capability.matches(path));
        window
            .active
            .retain(|active| !active.capability.matches(path));
        Ok(before != window.pending_native_grants.len() + window.active.len())
    }

    pub(crate) fn discard_pending_root(
        &self,
        window_label: &str,
        path: &Path,
    ) -> Result<bool, FsError> {
        let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        let Some(window) = state.windows.get_mut(window_label) else {
            return Ok(false);
        };
        let before = window.pending_native_grants.len();
        window
            .pending_native_grants
            .retain(|pending| !pending.capability.matches_root(path));
        Ok(before != window.pending_native_grants.len())
    }

    pub(crate) fn revoke_window(&self, window_label: &str) -> Result<usize, FsError> {
        let mut state = self.inner.lock().map_err(|_| FsError::StateUnavailable)?;
        Ok(state
            .windows
            .remove(window_label)
            .map_or(0, |window| window.active.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::{ExternalPathGrantRegistry, MAX_NATIVE_DROP_PATHS};
    use mcode_fs::{BoundedFileRead, ExactPathCapability};
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
    fn claims_require_a_captured_native_object_for_the_same_window() {
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
    fn native_stage_pins_object_identity_before_renderer_claim() {
        let root = TestDirectory::new();
        let target = root.path().join("external.txt");
        let original = root.path().join("original.txt");
        fs::write(&target, "captured").unwrap();
        let registry = ExternalPathGrantRegistry::default();
        registry
            .stage_native_paths("main", std::slice::from_ref(&target))
            .unwrap();

        fs::rename(&target, &original).unwrap();
        fs::write(&target, "replacement").unwrap();
        assert!(registry.claim("main", &target).unwrap());
        let capability = registry.capability_for("main", &target).unwrap().unwrap();

        assert_eq!(
            capability.read_file_bounded(&target, 32, None).unwrap(),
            BoundedFileRead::Bytes(b"captured".to_vec())
        );
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
    fn retained_grants_use_one_global_lru_across_windows() {
        let root = TestDirectory::new();
        let paths = ["a.txt", "b.txt", "c.txt"].map(|name| root.path().join(name));
        for path in &paths {
            fs::write(path, "value").unwrap();
        }
        let registry = ExternalPathGrantRegistry::with_limit(2);

        stage_and_claim(&registry, "window-a", &paths[0]);
        stage_and_claim(&registry, "window-b", &paths[1]);
        assert!(
            registry
                .capability_for("window-a", &paths[0])
                .unwrap()
                .is_some()
        );
        stage_and_claim(&registry, "window-c", &paths[2]);

        assert!(
            registry
                .capability_for("window-a", &paths[0])
                .unwrap()
                .is_some()
        );
        assert!(
            registry
                .capability_for("window-b", &paths[1])
                .unwrap()
                .is_none()
        );
        assert!(
            registry
                .capability_for("window-c", &paths[2])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn claiming_a_dropped_child_keeps_an_active_parent_directory_grant() {
        let root = TestDirectory::new();
        let directory = root.path().join("directory");
        let child = directory.join("child.txt");
        fs::create_dir(&directory).unwrap();
        fs::write(&child, "child").unwrap();
        let registry = ExternalPathGrantRegistry::default();

        stage_and_claim(&registry, "main", &directory);
        stage_and_claim(&registry, "main", &child);

        let state = registry.inner.lock().unwrap();
        let active = &state.windows.get("main").unwrap().active;
        assert_eq!(active.len(), 2);
        assert!(
            active
                .iter()
                .any(|grant| grant.capability.matches_root(&directory))
        );
        assert!(
            active
                .iter()
                .any(|grant| grant.capability.matches_root(&child))
        );
    }

    #[test]
    fn oversized_native_drop_is_rejected_before_any_capture() {
        let registry = ExternalPathGrantRegistry::default();
        let paths = (0..=MAX_NATIVE_DROP_PATHS)
            .map(|index| PathBuf::from(format!("C:\\oversized\\{index}.txt")))
            .collect::<Vec<_>>();

        let error = registry
            .stage_native_paths_with("main", &paths, |_| {
                panic!("oversized drops must not capture filesystem objects")
            })
            .unwrap_err();

        assert_eq!(error.code(), "fs_external_too_many_paths");
        assert!(registry.inner.lock().unwrap().windows.is_empty());
    }

    #[test]
    fn pending_native_handles_also_count_toward_the_global_budget() {
        let root = TestDirectory::new();
        let paths = ["a.txt", "b.txt", "c.txt"].map(|name| root.path().join(name));
        for path in &paths {
            fs::write(path, "value").unwrap();
        }
        let registry = ExternalPathGrantRegistry::with_limit(2);

        for (index, path) in paths.iter().enumerate() {
            registry
                .stage_native_paths(&format!("window-{index}"), std::slice::from_ref(path))
                .unwrap();
        }

        assert!(!registry.claim("window-0", &paths[0]).unwrap());
        assert!(registry.claim("window-1", &paths[1]).unwrap());
        assert!(registry.claim("window-2", &paths[2]).unwrap());
    }

    #[test]
    fn capture_reserves_global_capacity_before_opening_another_handle() {
        let root = TestDirectory::new();
        let paths = ["a.txt", "b.txt", "c.txt"].map(|name| root.path().join(name));
        for path in &paths {
            fs::write(path, "value").unwrap();
        }
        let registry = ExternalPathGrantRegistry::with_limit(2);
        registry
            .stage_native_paths("window-a", std::slice::from_ref(&paths[0]))
            .unwrap();
        registry
            .stage_native_paths("window-b", std::slice::from_ref(&paths[1]))
            .unwrap();
        let inspected_registry = registry.clone();

        registry
            .stage_native_paths_with("window-c", std::slice::from_ref(&paths[2]), |path| {
                let state = inspected_registry.inner.lock().unwrap();
                assert_eq!(state.retained_grant_count(), 1);
                assert_eq!(state.capture_reservations, 1);
                drop(state);
                ExactPathCapability::authorize(path)
            })
            .unwrap();

        assert!(!registry.claim("window-a", &paths[0]).unwrap());
        assert!(registry.claim("window-b", &paths[1]).unwrap());
        assert!(registry.claim("window-c", &paths[2]).unwrap());
    }

    #[test]
    fn revoking_a_window_removes_state_and_label_reuse_gets_a_unique_generation() {
        let root = TestDirectory::new();
        let target = root.path().join("external.txt");
        fs::write(&target, "external").unwrap();
        let registry = ExternalPathGrantRegistry::default();
        stage_and_claim(&registry, "main", &target);
        let first_generation = registry
            .inner
            .lock()
            .unwrap()
            .windows
            .get("main")
            .unwrap()
            .generation;

        assert_eq!(registry.revoke_window("main").unwrap(), 1);
        assert!(!registry.inner.lock().unwrap().windows.contains_key("main"));
        registry
            .stage_native_paths("main", std::slice::from_ref(&target))
            .unwrap();
        let replacement_generation = registry
            .inner
            .lock()
            .unwrap()
            .windows
            .get("main")
            .unwrap()
            .generation;

        assert!(replacement_generation > first_generation);
        assert!(registry.claim("main", &target).unwrap());
    }

    #[test]
    fn capture_finishing_after_window_replacement_cannot_stage_into_reused_label() {
        let root = TestDirectory::new();
        let stale = root.path().join("stale.txt");
        let current = root.path().join("current.txt");
        fs::write(&stale, "stale").unwrap();
        fs::write(&current, "current").unwrap();
        let registry = ExternalPathGrantRegistry::default();
        let capture_started = Arc::new(Barrier::new(2));
        let continue_capture = Arc::new(Barrier::new(2));
        let stage_registry = registry.clone();
        let stage_path = stale.clone();
        let started = capture_started.clone();
        let resume = continue_capture.clone();

        let stale_stage = std::thread::spawn(move || {
            stage_registry.stage_native_paths_with("main", &[stage_path], |path| {
                started.wait();
                resume.wait();
                ExactPathCapability::authorize(path)
            })
        });
        capture_started.wait();
        assert_eq!(registry.revoke_window("main").unwrap(), 0);
        registry
            .stage_native_paths("main", std::slice::from_ref(&current))
            .unwrap();
        continue_capture.wait();
        stale_stage.join().unwrap().unwrap();

        assert!(!registry.claim("main", &stale).unwrap());
        assert!(registry.claim("main", &current).unwrap());
    }
}
