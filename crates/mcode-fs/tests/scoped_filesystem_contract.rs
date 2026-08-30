use mcode_fs::{FsError, PathKind, ScopedFilesystem};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mcode-fs-contract-{}-{sequence}",
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

#[test]
fn registered_root_lists_directories_before_naturally_sorted_files() {
    let root = TestDirectory::new();
    fs::create_dir(root.path().join("folder10")).unwrap();
    fs::create_dir(root.path().join("folder2")).unwrap();
    fs::write(root.path().join("file10.txt"), "ten").unwrap();
    fs::write(root.path().join("file2.txt"), "two").unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    let entries = filesystem.read_dir(root.path()).unwrap();

    assert_eq!(
        entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
            .collect::<Vec<_>>(),
        vec![
            ("folder2", PathKind::Directory),
            ("folder10", PathKind::Directory),
            ("file2.txt", PathKind::File),
            ("file10.txt", PathKind::File),
        ]
    );
}

#[test]
fn target_paths_must_be_normalized_absolute_and_inside_a_registered_root() {
    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert!(matches!(
        filesystem.path_exists(Path::new("relative.txt")),
        Err(FsError::InvalidTargetPath { .. })
    ));
    assert!(matches!(
        filesystem.path_exists(&root.path().join("folder").join("..").join("file.txt")),
        Err(FsError::InvalidTargetPath { .. })
    ));
    assert!(matches!(
        filesystem.path_exists(outside.path()),
        Err(FsError::PathOutsideScope { .. })
    ));
}

#[test]
fn missing_leaf_uses_nearest_existing_ancestor_without_leaving_the_scope() {
    let root = TestDirectory::new();
    fs::create_dir(root.path().join("existing")).unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert!(
        !filesystem
            .path_exists(&root.path().join("existing/missing/nested.txt"))
            .unwrap()
    );
}

#[test]
fn stat_and_path_exists_return_file_metadata() {
    let root = TestDirectory::new();
    let file = root.path().join("notes.txt");
    fs::write(&file, "hello").unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert!(filesystem.path_exists(&file).unwrap());
    let metadata = filesystem.stat(&file).unwrap();
    assert_eq!(metadata.kind, PathKind::File);
    assert_eq!(metadata.size, 5);
    assert!(metadata.modified_time_millis.is_finite());
}

#[test]
fn unregistering_a_scope_revokes_its_root_and_unknown_ids_are_typed() {
    let root = TestDirectory::new();
    let filesystem = ScopedFilesystem::new();
    let scope_id = filesystem.register_root(root.path()).unwrap();

    filesystem.unregister_scope(scope_id).unwrap();
    assert!(matches!(
        filesystem.path_exists(root.path()),
        Err(FsError::PathOutsideScope { .. })
    ));
    assert!(matches!(
        filesystem.unregister_scope(scope_id),
        Err(FsError::UnknownScope { .. })
    ));
}

#[test]
fn replacing_roots_atomically_revokes_old_roots_and_skips_stale_roots() {
    let old_root = TestDirectory::new();
    let new_root = TestDirectory::new();
    let stale_root = new_root.path().join("missing-root");
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(old_root.path()).unwrap();

    let scope_ids = filesystem
        .replace_roots([new_root.path(), stale_root.as_path()])
        .unwrap();

    assert_eq!(scope_ids.len(), 1);
    assert!(matches!(
        filesystem.path_exists(old_root.path()),
        Err(FsError::PathOutsideScope { .. })
    ));
    assert!(filesystem.path_exists(new_root.path()).unwrap());
}

#[test]
fn natural_sort_is_stable_for_numeric_ties() {
    let root = TestDirectory::new();
    for name in ["file10", "file02", "file2", "File1"] {
        fs::write(root.path().join(name), name).unwrap();
    }
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    let names = filesystem
        .read_dir(root.path())
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect::<Vec<_>>();

    assert_eq!(names, ["File1", "file02", "file2", "file10"]);
}

#[cfg(any(unix, windows))]
#[test]
fn leaf_symlink_to_an_inside_target_is_followed_by_public_operations() {
    let root = TestDirectory::new();
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("visible.txt"), "visible").unwrap();
    let link = root.path().join("inside-link");
    create_directory_symlink(&target, &link);
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    let entries = filesystem.read_dir(&link).unwrap();
    assert_eq!(entries[0].name, "visible.txt");
    assert_eq!(filesystem.stat(&link).unwrap().kind, PathKind::Directory);
    assert!(filesystem.path_exists(&link).unwrap());
}

#[cfg(any(unix, windows))]
#[test]
fn intermediate_symlink_to_an_inside_target_is_followed_by_public_operations() {
    let root = TestDirectory::new();
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("visible.txt"), "visible").unwrap();
    let link = root.path().join("inside-link");
    create_directory_symlink(Path::new("target"), &link);
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();
    let file_through_link = link.join("visible.txt");

    assert!(filesystem.path_exists(&file_through_link).unwrap());
    assert_eq!(
        filesystem.stat(&file_through_link).unwrap().kind,
        PathKind::File
    );
}

#[cfg(any(unix, windows))]
#[test]
fn absolute_inside_symlink_targets_are_followed_after_os_normalization() {
    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    fs::create_dir(outside.path().join("a")).unwrap();
    fs::create_dir(outside.path().join("target")).unwrap();
    fs::write(outside.path().join("target/secret.txt"), "secret").unwrap();
    fs::create_dir(root.path().join("target")).unwrap();
    fs::write(root.path().join("target/visible.txt"), "visible").unwrap();
    create_directory_symlink(&outside.path().join("a"), &root.path().join("a"));
    create_directory_symlink(
        &root.path().join("a/../target"),
        &root.path().join("normalized-link"),
    );
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    let link = root.path().join("normalized-link");
    let stored_target = fs::read_link(&link).unwrap();
    let result = filesystem.read_dir(&link);
    if stored_target
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        assert!(matches!(result, Err(FsError::PathOutsideScope { .. })));
    } else {
        let entries = result.unwrap();
        assert_eq!(entries[0].name, "visible.txt");
    }
}

#[cfg(any(unix, windows))]
#[test]
fn leaf_symlink_to_an_outside_target_is_denied_by_public_operations() {
    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    let link = root.path().join("outside-link");
    create_directory_symlink(outside.path(), &link);
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert!(matches!(
        filesystem.read_dir(&link),
        Err(FsError::PathOutsideScope { .. })
    ));
    assert!(matches!(
        filesystem.stat(&link),
        Err(FsError::PathOutsideScope { .. })
    ));
    assert!(matches!(
        filesystem.path_exists(&link),
        Err(FsError::PathOutsideScope { .. })
    ));
}

#[cfg(any(unix, windows))]
#[test]
fn dangling_leaf_symlink_is_not_found_for_stat_and_path_exists() {
    let root = TestDirectory::new();
    let link = root.path().join("dangling-link");
    create_directory_symlink(Path::new("missing-target"), &link);
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert!(matches!(
        filesystem.stat(&link),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
    assert!(!filesystem.path_exists(&link).unwrap());
}

#[cfg(any(unix, windows))]
#[test]
fn concurrent_directory_replacement_never_exposes_outside_entries() {
    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    let active = root.path().join("active");
    fs::create_dir(&active).unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let successful_replacements = Arc::new(AtomicU64::new(0));

    std::thread::scope(|scope| {
        let reader_filesystem = filesystem.clone();
        let reader_active = active.clone();
        let reader_finished = Arc::clone(&finished);
        scope.spawn(move || {
            let mut reads = 0;
            while !reader_finished.load(Ordering::Acquire) || reads < 100 {
                match reader_filesystem.read_dir(&reader_active) {
                    Ok(entries) => assert!(entries.iter().all(|entry| entry.name != "secret.txt")),
                    Err(FsError::PathOutsideScope { .. }) => {}
                    Err(FsError::Io { .. }) => {}
                    Err(error) => panic!("unexpected replacement-race result: {error}"),
                }
                reads += 1;
                std::thread::yield_now();
            }
        });

        for sequence in 0..500 {
            let safe = root.path().join(format!("safe-{sequence}"));
            let escape = root.path().join(format!("escape-{sequence}"));
            fs::create_dir(&safe).unwrap();
            create_directory_symlink(outside.path(), &escape);
            if remove_directory_or_symlink(&active).is_ok() && fs::rename(&escape, &active).is_ok()
            {
                successful_replacements.fetch_add(1, Ordering::Relaxed);
                std::thread::yield_now();
                let _ = remove_directory_or_symlink(&active);
            }
            if !active.exists() {
                let _ = fs::rename(&safe, &active);
            }
            let _ = remove_directory_or_symlink(&safe);
            let _ = remove_directory_or_symlink(&escape);
        }
        finished.store(true, Ordering::Release);
    });

    assert!(successful_replacements.load(Ordering::Relaxed) > 0);
}

#[cfg(any(unix, windows))]
fn remove_directory_or_symlink(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(path),
        Ok(metadata) if metadata.is_dir() => fs::remove_dir(path),
        Ok(_) => fs::remove_file(path),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) {
    std::os::windows::fs::symlink_dir(target, link).unwrap();
}
