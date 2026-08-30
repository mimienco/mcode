//! Authorization unit tests (spec §2 invariants).

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use mcode_fs::scope::{FsError, FsScopeRegistry, LeafPolicy};

#[test]
fn inside_root_authorizes() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "hi").unwrap();
    let authorized = registry.authorize(&file, LeafPolicy::Canonicalize).unwrap();
    assert!(
        authorized
            .as_path()
            .starts_with(registry.roots()[0].as_path())
    );
}

#[test]
fn outside_root_denied() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    let outside = TempDir::new().unwrap();
    let error = registry
        .authorize(outside.path(), LeafPolicy::Canonicalize)
        .unwrap_err();
    assert_eq!(error, FsError::PathDenied);
    assert_eq!(error.code(), "fs_path_denied");
}

#[test]
fn traversal_denied() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    // A sibling path reached via .. stays denied.
    let sibling = dir.path().parent().unwrap().join("not-the-root");
    let error = registry
        .authorize(&sibling, LeafPolicy::Canonicalize)
        .unwrap_err();
    assert_eq!(error.code(), "fs_path_denied");
}

#[cfg(unix)]
#[test]
fn symlink_leaf_preserved() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();

    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, "sneaky").unwrap();

    let link = dir.path().join("link.txt");
    std::os::unix::fs::symlink(&secret, &link).unwrap();

    // Preserve keeps the link itself (inside root) even though the target is
    // outside; canonicalize dereferences to the outside target and is denied.
    let preserved = registry
        .authorize(&link, LeafPolicy::Preserve)
        .expect("preserve must stay inside root");
    assert_eq!(preserved.as_path(), link);

    let canonical = registry.authorize(&link, LeafPolicy::Canonicalize);
    assert!(
        canonical.is_err(),
        "dereferenced symlink target must be denied"
    );
}

#[test]
fn missing_leaf_falls_back_to_ancestor() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    let nested = dir.path().join("new-dir").join("new-file.txt");
    let authorized = registry
        .authorize(&nested, LeafPolicy::Canonicalize)
        .unwrap();
    // The resolved path is canonical (Windows verbatim prefix included) and
    // keeps the missing tail attached under the existing ancestor.
    assert!(authorized.as_path().ends_with("new-dir\\new-file.txt"));
}

#[test]
fn unregistered_root_denied_with_worktree_code() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    // Nothing registered at all.
    let error = registry
        .authorize(dir.path(), LeafPolicy::Canonicalize)
        .unwrap_err();
    assert_eq!(error.code(), "fs_path_denied");
}

#[test]
fn external_path_cache_roundtrip() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    let file = dir.path().join("cached.txt");
    fs::write(&file, "x").unwrap();

    assert!(!registry.is_externally_authorized(&file));
    registry.authorize_external_path(&file).unwrap();
    assert!(registry.is_externally_authorized(&file));
}

#[test]
fn unregister_root_revokes_access() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    assert!(
        registry
            .authorize(dir.path(), LeafPolicy::Canonicalize)
            .is_ok()
    );
    registry.unregister_root(dir.path());
    assert_eq!(
        registry
            .authorize(dir.path(), LeafPolicy::Canonicalize)
            .unwrap_err()
            .code(),
        "fs_path_denied"
    );
}

#[test]
fn error_display_and_codes_are_stable() {
    assert_eq!(FsError::WorktreeUnknown.code(), "fs_worktree_unknown");
    assert_eq!(FsError::NotFound.code(), "fs_not_found");
    assert!(FsError::NotFound.is_enoent());
    assert!(FsError::Io("ENOENT: no such file or directory".into()).is_enoent());
    assert!(FsError::Io("os error 2".into()).is_enoent());
    assert!(!FsError::Io("os error 5".into()).is_enoent());
}

#[test]
fn error_display_is_not_a_code() {
    // Display carries the human message; code() the stable identifier.
    let error = FsError::PathDenied;
    assert_ne!(error.to_string(), error.code());
    assert!(error.to_string().contains("Access denied"));
}

#[test]
fn registry_default_is_empty() {
    let registry = FsScopeRegistry::default();
    assert!(registry.roots().is_empty());
}

#[test]
fn registered_root_returns_authorized_handle() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    let root = registry.register_root(dir.path()).unwrap();
    assert_eq!(root.as_path(), dir.path().canonicalize().unwrap());
}

#[test]
fn authorize_accepts_canonical_policy_for_real_file() {
    let dir = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(dir.path()).unwrap();
    let file = dir.path().join("real.txt");
    fs::write(&file, "abc").unwrap();
    let authorized = registry
        .authorize(Path::new(&file), LeafPolicy::Canonicalize)
        .unwrap();
    assert!(authorized.as_path().ends_with("real.txt"));
}
// ---------- Invariants ported from the cap-std contract suite (1c3d7316dc) ----------

#[cfg(any(unix, windows))]
fn create_directory_symlink(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(target, link).unwrap();
}

/// A symlink leaf pointing at a directory outside every root must be denied
/// under Canonicalize (the dereferenced target is outside); under Preserve
/// the link itself — inside the root — is returned (spec §2).
#[cfg(any(unix, windows))]
#[test]
fn directory_symlink_leaf_to_outside_target_is_denied() {
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let link = root.path().join("outside-link");
    create_directory_symlink(outside.path(), &link);
    let registry = FsScopeRegistry::new();
    registry.register_root(root.path()).unwrap();

    let canonical = registry.authorize(&link, LeafPolicy::Canonicalize);
    assert_eq!(canonical.unwrap_err().code(), "fs_path_denied");

    // Preserve keeps the leaf link path (still inside root).
    let preserved = registry
        .authorize(&link, LeafPolicy::Preserve)
        .expect("preserve must keep the in-root link leaf");
    assert!(preserved.as_path().ends_with("outside-link"));
}

/// A dangling symlink leaf resolves to NotFound for stat-style operations and
/// `path_exists` reports false rather than erroring.
#[cfg(any(unix, windows))]
#[test]
fn dangling_leaf_symlink_is_not_found() {
    let root = TempDir::new().unwrap();
    let link = root.path().join("dangling-link");
    create_directory_symlink(Path::new("missing-target"), &link);
    let registry = FsScopeRegistry::new();
    registry.register_root(root.path()).unwrap();

    let authorized = registry
        .authorize(&link, LeafPolicy::Canonicalize)
        .expect("dangling in-root link stays inside the root");
    let error = mcode_fs::ops::stat(&authorized).unwrap_err();
    assert_eq!(error.code(), "fs_not_found");
    assert!(!mcode_fs::ops::path_exists(&authorized).unwrap());
}

/// A symlink leaf pointing at a target inside the same root authorizes to the
/// resolved target for public operations (Canonicalize follows it).
#[cfg(any(unix, windows))]
#[test]
fn leaf_symlink_to_inside_target_is_followed() {
    let root = TempDir::new().unwrap();
    let real_dir = root.path().join("real-dir");
    std::fs::create_dir_all(&real_dir).unwrap();
    let link = root.path().join("inside-link");
    create_directory_symlink(&real_dir, &link);
    let registry = FsScopeRegistry::new();
    registry.register_root(root.path()).unwrap();

    let authorized = registry
        .authorize(&link, LeafPolicy::Canonicalize)
        .expect("inside target resolves inside the root");
    assert!(authorized.as_path().ends_with("real-dir"));
}

/// Target paths reaching outside the root via `..` traversal are denied even
/// when intermediate components exist (spec §2 containment).
#[test]
fn traversal_with_existing_intermediates_denied() {
    let root = TempDir::new().unwrap();
    let registry = FsScopeRegistry::new();
    registry.register_root(root.path()).unwrap();
    std::fs::create_dir_all(root.path().join("a").join("b")).unwrap();
    let escape = root
        .path()
        .join("a")
        .join("b")
        .join("..")
        .join("..")
        .join("..");
    let error = registry
        .authorize(&escape, LeafPolicy::Canonicalize)
        .unwrap_err();
    assert_eq!(error.code(), "fs_path_denied");
}
