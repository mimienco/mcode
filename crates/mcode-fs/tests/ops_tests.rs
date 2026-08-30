//! Operation tests over authorized paths (spec §4 semantics).

use std::fs;

use tempfile::TempDir;

use mcode_fs::ops;
use mcode_fs::scope::{FsScopeRegistry, LeafPolicy};

struct Ctx {
    dir: TempDir,
    registry: FsScopeRegistry,
}

impl Ctx {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let registry = FsScopeRegistry::new();
        registry.register_root(dir.path()).unwrap();
        Self { dir, registry }
    }

    fn auth(&self, relative: &str) -> mcode_fs::scope::AuthorizedPath {
        self.registry
            .authorize(&self.dir.path().join(relative), LeafPolicy::Canonicalize)
            .expect("authorization inside root")
    }
}

#[test]
fn write_then_read_roundtrip_text() {
    let ctx = Ctx::new();
    let path = ctx.auth("hello.txt");
    ops::write_file(&path, "hello world").unwrap();
    let result = ops::read_file(&path).unwrap();
    assert_eq!(result.content, "hello world");
    assert!(!result.is_binary);
}

#[test]
fn read_binary_returns_base64() {
    let ctx = Ctx::new();
    let path = ctx.auth("blob.bin");
    ops::write_file(&path, "abc\0def").unwrap();
    let result = ops::read_file(&path).unwrap();
    assert!(result.is_binary);
    // base64 of "abc\0def"
    assert_eq!(result.content, "YWJjAGRlZg==");
}

#[test]
fn read_missing_file_is_not_found() {
    let ctx = Ctx::new();
    let path = ctx.auth("ghost.txt");
    let error = ops::read_file(&path).unwrap_err();
    assert_eq!(error.code(), "fs_not_found");
}

#[test]
fn read_directory_is_rejected() {
    let ctx = Ctx::new();
    let path = ctx.auth("subdir");
    ops::create_dir(&path).unwrap();
    let error = ops::read_file(&path).unwrap_err();
    assert_eq!(error.code(), "fs_is_directory");
}

#[test]
fn write_to_directory_rejected() {
    let ctx = Ctx::new();
    let dir_path = ctx.auth("adir");
    ops::create_dir(&dir_path).unwrap();
    let error = ops::write_file(&dir_path, "nope").unwrap_err();
    assert_eq!(error.code(), "fs_is_directory");
}

#[test]
fn atomic_write_is_all_or_nothing() {
    let ctx = Ctx::new();
    let path = ctx.auth("atomic.txt");
    ops::write_file(&path, "old content").unwrap();
    ops::write_file_atomic(&path, "new content").unwrap();
    let result = ops::read_file(&path).unwrap();
    assert_eq!(result.content, "new content");
    // No temp litter left behind.
    let leftovers: Vec<_> = fs::read_dir(ctx.dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn atomic_write_missing_parent_creates_it() {
    let ctx = Ctx::new();
    let path = ctx.auth("deep/nested/dir/file.txt");
    ops::write_file_atomic(&path, "content").unwrap();
    assert_eq!(ops::read_file(&path).unwrap().content, "content");
}

#[test]
fn create_file_is_create_new() {
    let ctx = Ctx::new();
    let path = ctx.auth("fresh.txt");
    ops::create_file(&path).unwrap();
    let error = ops::create_file(&path).unwrap_err();
    assert_eq!(error.code(), "fs_already_exists");
}

#[test]
fn copy_file_no_clobber() {
    let ctx = Ctx::new();
    let source = ctx.auth("src.txt");
    ops::write_file(&source, "payload").unwrap();
    let dest = ctx.auth("dest.txt");
    ops::write_file(&dest, "existing").unwrap();

    let error = ops::copy_path(&source, &dest).unwrap_err();
    assert_eq!(error.code(), "fs_destination_race");
    // Dest content untouched by the failed copy.
    assert_eq!(ops::read_file(&dest).unwrap().content, "existing");
}

#[test]
fn copy_file_success_and_content() {
    let ctx = Ctx::new();
    let source = ctx.auth("src.txt");
    ops::write_file(&source, "payload").unwrap();
    let dest = ctx.auth("dest.txt");
    ops::copy_path(&source, &dest).unwrap();
    assert_eq!(ops::read_file(&dest).unwrap().content, "payload");
}

#[test]
fn copy_dir_recursive_no_clobber() {
    let ctx = Ctx::new();
    let source_dir = ctx.auth("tree");
    ops::create_dir(&source_dir).unwrap();
    ops::write_file(&ctx.auth("tree/a.txt"), "A").unwrap();
    ops::create_dir(&ctx.auth("tree/nested")).unwrap();
    ops::write_file(&ctx.auth("tree/nested/b.txt"), "B").unwrap();

    let dest_dir = ctx.auth("tree-copy");
    ops::copy_path(&source_dir, &dest_dir).unwrap();
    assert_eq!(
        ops::read_file(&ctx.auth("tree-copy/a.txt"))
            .unwrap()
            .content,
        "A"
    );
    assert_eq!(
        ops::read_file(&ctx.auth("tree-copy/nested/b.txt"))
            .unwrap()
            .content,
        "B"
    );

    // Re-copy onto the existing tree must fail, not merge.
    let error = ops::copy_path(&source_dir, &dest_dir).unwrap_err();
    assert_eq!(error.code(), "fs_destination_race");
}

#[test]
fn rename_no_clobber() {
    let ctx = Ctx::new();
    let source = ctx.auth("from.txt");
    ops::write_file(&source, "moving").unwrap();
    let dest = ctx.auth("to.txt");
    ops::write_file(&dest, "occupied").unwrap();

    let error = ops::rename_path(&source, &dest).unwrap_err();
    assert_eq!(error.code(), "fs_destination_race");
    assert_eq!(ops::read_file(&dest).unwrap().content, "occupied");
    // Source still present.
    assert_eq!(ops::read_file(&source).unwrap().content, "moving");
}

#[test]
fn rename_success() {
    let ctx = Ctx::new();
    let source = ctx.auth("from.txt");
    ops::write_file(&source, "moving").unwrap();
    let dest = ctx.auth("to.txt");
    ops::rename_path(&source, &dest).unwrap();
    assert_eq!(ops::read_file(&dest).unwrap().content, "moving");
    assert!(!ops::path_exists(&source).unwrap());
}

#[test]
fn delete_is_idempotent_on_missing_path() {
    let ctx = Ctx::new();
    let path = ctx.auth("never-existed.txt");
    ops::delete_path(&path).unwrap(); // ENOENT swallowed
    ops::delete_path(&path).unwrap(); // still ok
}

#[test]
fn delete_file_removes_it() {
    let ctx = Ctx::new();
    let path = ctx.auth("doomed.txt");
    ops::write_file(&path, "x").unwrap();
    ops::delete_path(&path).unwrap();
    // Either the Recycle Bin took it (gone from place) or hard delete; the
    // path must no longer exist either way.
    assert!(!ops::path_exists(&path).unwrap());
}

#[test]
fn stat_reports_size_and_type() {
    let ctx = Ctx::new();
    let path = ctx.auth("measured.txt");
    ops::write_file(&path, "12345").unwrap();
    let stat = ops::stat(&path).unwrap();
    assert_eq!(stat.size, 5);
    assert!(!stat.is_directory);

    let dir = ctx.auth("dirdir");
    ops::create_dir(&dir).unwrap();
    let dir_stat = ops::stat(&dir).unwrap();
    assert!(dir_stat.is_directory);
}

#[test]
fn path_exists_false_only_for_missing() {
    let ctx = Ctx::new();
    let missing = ctx.auth("nothing-here.txt");
    assert!(!ops::path_exists(&missing).unwrap());
    let present = ctx.auth("present.txt");
    ops::write_file(&present, "y").unwrap();
    assert!(ops::path_exists(&present).unwrap());
}

#[test]
fn read_dir_reports_names_and_types() {
    let ctx = Ctx::new();
    ops::create_dir(&ctx.auth("dirlisting")).unwrap();
    ops::write_file(&ctx.auth("dirlisting/file.txt"), "x").unwrap();
    ops::create_dir(&ctx.auth("dirlisting/sub")).unwrap();
    let entries = ops::read_dir(&ctx.auth("dirlisting")).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["file.txt", "sub"]);
    let file_entry = &entries[0];
    assert!(!file_entry.is_directory && !file_entry.is_symlink);
    let dir_entry = &entries[1];
    assert!(dir_entry.is_directory && !dir_entry.is_symlink);
}

#[test]
fn read_file_too_large() {
    let ctx = Ctx::new();
    let path = ctx.auth("big.txt");
    // Smuggle an oversized file in directly (bypassing the 50 MB write path).
    let big = ctx.dir.path().join("big.txt");
    let file = fs::File::create(&big).unwrap();
    file.set_len(51 * 1024 * 1024).unwrap();
    drop(file);
    let error = ops::read_file(&path).unwrap_err();
    assert_eq!(error.code(), "fs_too_large");
}

#[test]
fn create_dir_makes_parents() {
    let ctx = Ctx::new();
    let path = ctx.auth("a/b/c");
    ops::create_dir(&path).unwrap();
    assert!(ops::path_exists(&path).unwrap());
}
