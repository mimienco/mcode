use mcode_fs::{BoundedFileRead, ExactPathCapability, FsError, PathKind, ScopedFilesystem};
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
fn bounded_file_reads_return_bytes_detect_binary_prefixes_and_reject_oversized_files() {
    let root = TestDirectory::new();
    let text_path = root.path().join("notes.txt");
    let binary_path = root.path().join("archive.bin");
    fs::write(&text_path, "hello 🌍").unwrap();
    fs::write(&binary_path, [b'a', 0, b'b']).unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert_eq!(
        filesystem
            .read_file_bounded(&text_path, 32, Some(8))
            .unwrap(),
        BoundedFileRead::Bytes("hello 🌍".as_bytes().to_vec())
    );
    assert_eq!(
        filesystem
            .read_file_bounded(&binary_path, 32, Some(8))
            .unwrap(),
        BoundedFileRead::BinaryPrefix
    );
    assert!(matches!(
        filesystem.read_file_bounded(&text_path, 4, Some(4)),
        Err(FsError::FileTooLarge { limit: 4, .. })
    ));
}

#[test]
fn bounded_reads_enforce_byte_limits_at_multibyte_utf8_boundaries() {
    let root = TestDirectory::new();
    let file_path = root.path().join("unicode.txt");
    fs::write(&file_path, "🌍").unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert_eq!(
        filesystem.read_file_bounded(&file_path, 4, None).unwrap(),
        BoundedFileRead::Bytes("🌍".as_bytes().to_vec())
    );
    assert!(matches!(
        filesystem.read_file_bounded(&file_path, 3, None),
        Err(FsError::FileTooLarge { limit: 3, .. })
    ));
}

#[test]
fn bounded_snapshots_keep_identity_across_growth_and_change_it_after_atomic_replacement() {
    let root = TestDirectory::new();
    let file_path = root.path().join("live.log");
    fs::write(&file_path, "first").unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    let first = filesystem
        .read_file_snapshot_bounded(&file_path, 64, None)
        .unwrap();
    fs::write(&file_path, "first\nsecond").unwrap();
    let appended = filesystem
        .read_file_snapshot_bounded(&file_path, 64, None)
        .unwrap();
    filesystem
        .write_file_atomic(&file_path, b"replacement", 64)
        .unwrap();
    let replaced = filesystem
        .read_file_snapshot_bounded(&file_path, 64, None)
        .unwrap();

    assert_eq!(first.file_identity, appended.file_identity);
    assert_ne!(first.file_identity, replaced.file_identity);
}

#[test]
fn atomic_write_creates_and_replaces_without_leaving_temporary_files() {
    let root = TestDirectory::new();
    let file_path = root.path().join("notes.txt");
    fs::write(&file_path, "old").unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    filesystem
        .write_file_atomic(&file_path, b"new", 16)
        .unwrap();
    let created_path = root.path().join("created.txt");
    filesystem
        .write_file_atomic(&created_path, "hello 🌍".as_bytes(), 16)
        .unwrap();

    assert_eq!(fs::read(&file_path).unwrap(), b"new");
    assert_eq!(fs::read(&created_path).unwrap(), "hello 🌍".as_bytes());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    assert!(matches!(
        filesystem.write_file_atomic(&file_path, b"too long", 3),
        Err(FsError::FileTooLarge { limit: 3, .. })
    ));
    assert_eq!(fs::read(&file_path).unwrap(), b"new");
}

#[test]
fn concurrent_readers_observe_only_complete_atomic_versions() {
    let root = TestDirectory::new();
    let file_path = root.path().join("state.txt");
    let old = vec![b'a'; 16 * 1024];
    let new = vec![b'b'; 16 * 1024];
    fs::write(&file_path, &old).unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    std::thread::scope(|scope| {
        let reader_filesystem = filesystem.clone();
        let reader_path = file_path.clone();
        let reader_old = old.clone();
        let reader_new = new.clone();
        scope.spawn(move || {
            for _ in 0..200 {
                let BoundedFileRead::Bytes(bytes) = reader_filesystem
                    .read_file_bounded(&reader_path, 32 * 1024, None)
                    .unwrap()
                else {
                    panic!("binary detection was disabled")
                };
                assert!(bytes == reader_old || bytes == reader_new);
            }
        });

        for index in 0..100 {
            let contents = if index % 2 == 0 { &new } else { &old };
            filesystem
                .write_file_atomic(&file_path, contents, 32 * 1024)
                .unwrap();
        }
    });
}

#[cfg(any(unix, windows))]
#[test]
fn atomic_write_rejects_directories_and_follows_contained_symlink_targets() {
    let root = TestDirectory::new();
    let real_file = root.path().join("real.txt");
    let link = root.path().join("linked.txt");
    fs::write(&real_file, "original").unwrap();
    create_file_symlink(&real_file, &link);
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    assert!(matches!(
        filesystem.write_file_atomic(root.path(), b"no", 8),
        Err(FsError::TargetIsDirectory { .. })
    ));
    filesystem
        .write_file_atomic(&link, b"replacement", 16)
        .unwrap();
    assert_eq!(fs::read_to_string(&real_file).unwrap(), "replacement");
    assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
}

#[cfg(unix)]
#[test]
fn atomic_write_preserves_restrictive_unix_permissions() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let root = TestDirectory::new();
    let file_path = root.path().join("secret.txt");
    fs::write(&file_path, "old").unwrap();
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600)).unwrap();
    let filesystem = ScopedFilesystem::new();
    filesystem.register_root(root.path()).unwrap();

    filesystem
        .write_file_atomic(&file_path, b"new secret", 32)
        .unwrap();

    assert_eq!(fs::read_to_string(&file_path).unwrap(), "new secret");
    assert_eq!(fs::metadata(file_path).unwrap().mode() & 0o777, 0o600);
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

#[test]
fn exact_file_capability_allows_only_the_authorized_file() {
    let root = TestDirectory::new();
    let allowed = root.path().join("allowed.txt");
    let sibling = root.path().join("sibling.txt");
    fs::write(&allowed, "allowed").unwrap();
    fs::write(&sibling, "sibling").unwrap();
    let capability = ExactPathCapability::authorize(&allowed).unwrap();

    assert_eq!(
        capability.read_file_bounded(&allowed, 32, None).unwrap(),
        BoundedFileRead::Bytes(b"allowed".to_vec())
    );
    assert!(matches!(
        capability.stat(&sibling),
        Err(FsError::PathOutsideScope { .. })
    ));
    let write_error = capability
        .write_file_atomic(&allowed, b"replacement", 32)
        .unwrap_err();
    assert_eq!(write_error.code(), "fs_external_write_unsupported");
    assert_eq!(fs::read_to_string(&allowed).unwrap(), "allowed");
}

#[test]
fn exact_directory_capability_grants_only_the_authorized_subtree() {
    let root = TestDirectory::new();
    let directory = root.path().join("external");
    let child = directory.join("child.txt");
    let outside_sibling = root.path().join("outside.txt");
    fs::create_dir(&directory).unwrap();
    fs::write(&child, "child").unwrap();
    fs::write(&outside_sibling, "outside").unwrap();
    let capability = ExactPathCapability::authorize(&directory).unwrap();
    assert!(capability.matches_root(&directory));
    assert!(!capability.matches_root(&child));

    assert_eq!(
        capability.read_dir(&directory).unwrap()[0].name,
        "child.txt"
    );
    assert_eq!(
        capability.read_file_bounded(&child, 32, None).unwrap(),
        BoundedFileRead::Bytes(b"child".to_vec())
    );
    let write_error = capability
        .write_file_atomic(&child, b"replacement", 32)
        .unwrap_err();
    assert_eq!(write_error.code(), "fs_external_write_unsupported");
    assert_eq!(fs::read_to_string(&child).unwrap(), "child");
    assert!(matches!(
        capability.read_file_bounded(&outside_sibling, 32, None),
        Err(FsError::PathOutsideScope { .. })
    ));
}

#[cfg(any(unix, windows))]
#[test]
fn exact_directory_capability_denies_symlink_escape() {
    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    let directory = root.path().join("external");
    let escape = directory.join("escape");
    fs::create_dir(&directory).unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    create_directory_symlink(outside.path(), &escape);
    let capability = ExactPathCapability::authorize(&directory).unwrap();

    assert!(matches!(
        capability.read_file_bounded(&escape.join("secret.txt"), 32, None),
        Err(FsError::PathOutsideScope { .. } | FsError::SymlinkTraversalDenied { .. })
    ));
}

#[cfg(unix)]
#[test]
fn exact_directory_capability_keeps_the_original_handle_after_root_retarget() {
    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    let directory = root.path().join("external");
    let pinned = root.path().join("pinned");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("safe.txt"), "safe").unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let capability = ExactPathCapability::authorize(&directory).unwrap();

    fs::rename(&directory, &pinned).unwrap();
    create_directory_symlink(outside.path(), &directory);

    assert_eq!(
        capability
            .read_file_bounded(&directory.join("safe.txt"), 32, None)
            .unwrap(),
        BoundedFileRead::Bytes(b"safe".to_vec())
    );
    assert!(matches!(
        capability.read_file_bounded(&directory.join("secret.txt"), 32, None),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
}

#[cfg(any(unix, windows))]
#[test]
fn exact_directory_capability_keeps_the_original_handle_after_alias_retarget() {
    let root = TestDirectory::new();
    let target = TestDirectory::new();
    let outside = TestDirectory::new();
    let alias = root.path().join("external");
    fs::write(target.path().join("safe.txt"), "safe").unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    create_directory_symlink(target.path(), &alias);
    let capability = ExactPathCapability::authorize(&alias).unwrap();

    fs::remove_dir(&alias)
        .or_else(|_| fs::remove_file(&alias))
        .unwrap();
    create_directory_symlink(outside.path(), &alias);

    assert_eq!(
        capability
            .read_file_bounded(&alias.join("safe.txt"), 32, None)
            .unwrap(),
        BoundedFileRead::Bytes(b"safe".to_vec())
    );
    assert!(matches!(
        capability.read_file_bounded(&alias.join("secret.txt"), 32, None),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
}

#[test]
fn exact_missing_leaf_capability_is_rejected() {
    let root = TestDirectory::new();
    let missing = root.path().join("missing.txt");

    assert!(matches!(
        ExactPathCapability::authorize(&missing),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
}

#[cfg(any(unix, windows))]
#[test]
fn exact_symlink_capability_accepts_the_alias_without_exposing_target_siblings() {
    let root = TestDirectory::new();
    let target_root = TestDirectory::new();
    let target = target_root.path().join("target.txt");
    let sibling = target_root.path().join("secret.txt");
    let alias = root.path().join("alias.txt");
    fs::write(&target, "target").unwrap();
    fs::write(&sibling, "secret").unwrap();
    create_file_symlink(&target, &alias);
    let capability = ExactPathCapability::authorize(&alias).unwrap();

    assert_eq!(
        capability.read_file_bounded(&alias, 32, None).unwrap(),
        BoundedFileRead::Bytes(b"target".to_vec())
    );
    assert_eq!(
        capability.read_file_bounded(&target, 32, None).unwrap(),
        BoundedFileRead::Bytes(b"target".to_vec())
    );
    fs::remove_file(&alias).unwrap();
    create_file_symlink(&sibling, &alias);
    assert_eq!(
        capability.read_file_bounded(&alias, 32, None).unwrap(),
        BoundedFileRead::Bytes(b"target".to_vec())
    );
    assert!(matches!(
        capability.read_file_bounded(&sibling, 32, None),
        Err(FsError::PathOutsideScope { .. })
    ));
}

#[cfg(any(unix, windows))]
#[test]
fn exact_file_capability_keeps_the_original_handle_after_leaf_retarget() {
    let root = TestDirectory::new();
    let allowed = root.path().join("allowed.txt");
    let pinned_original = root.path().join("pinned-original.txt");
    let sibling = root.path().join("sibling.txt");
    fs::write(&allowed, "allowed").unwrap();
    fs::write(&sibling, "sibling").unwrap();
    let capability = ExactPathCapability::authorize(&allowed).unwrap();

    fs::rename(&allowed, &pinned_original).unwrap();
    create_file_symlink(&sibling, &allowed);

    assert_eq!(
        capability.read_file_bounded(&allowed, 32, None).unwrap(),
        BoundedFileRead::Bytes(b"allowed".to_vec())
    );
    assert!(matches!(
        capability.read_file_bounded(&sibling, 32, None),
        Err(FsError::PathOutsideScope { .. })
    ));
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

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) {
    std::os::windows::fs::symlink_file(target, link).unwrap();
}
