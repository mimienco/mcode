//! Integration tests: real `git` binary on tempdirs. Self-contained — each
//! test initializes its own repositories. If no git binary exists on PATH,
//! tests print a loud warning and return Ok (skip) rather than fail.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use mcode_git::{CreatedWorktree, DiffFormat, GitRunner, RemoveForce, Repo, WorktreeService};
use tempfile::TempDir;

fn git_available() -> bool {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

macro_rules! require_git {
    () => {
        if !git_available() {
            eprintln!("=== WARNING: git binary not available; SKIPPING integration test (this is loud on purpose) ===");
            return Ok(());
        }
    };
}

fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(git_binary_path())
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test Author")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test Committer")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {args:?} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn git_binary_path() -> PathBuf {
    // The runner resolves through PATH; use plain "git" and let the OS find it.
    if cfg!(windows) {
        PathBuf::from("git.exe")
    } else {
        PathBuf::from("git")
    }
}

/// Fresh repo with one commit and clean tree.
struct TestRepo {
    _dir: TempDir,
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = TempDir::new().map_err(|e| e.to_string())?;
        let root = dir.path().join(name);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        run_git(&root, &["init", "--initial-branch=main"])?;
        run_git(&root, &["config", "user.email", "test@example.com"])?;
        run_git(&root, &["config", "user.name", "Test Author"])?;
        Ok(Self { _dir: dir, root })
    }

    fn repo(&self) -> Repo {
        Repo::discover(&self.root).expect("discovery on initialized repo")
    }

    fn write(&self, rel: &str, content: &str) -> Result<(), String> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, content).map_err(|e| e.to_string())
    }

    fn commit_all(&self, message: &str) -> Result<(), String> {
        run_git(&self.root, &["add", "."])?;
        run_git(&self.root, &["commit", "-m", message])?;
        Ok(())
    }
}

#[test]
fn stage_commit_status_roundtrip() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("roundtrip")?;
    repo.write("hello.txt", "world\n")?;

    // Initially untracked.
    let status = repo.repo().status(false).map_err(|e| e.to_string())?;
    assert_eq!(status.untracked, vec!["hello.txt"]);

    // Stage → appears staged.
    let repo_handle = repo.repo();
    let path = repo.root.join("hello.txt");
    repo_handle.stage(&[&path]).map_err(|e| e.to_string())?;
    let status = repo_handle.status(false).map_err(|e| e.to_string())?;
    assert_eq!(status.entries.len(), 1);
    assert_eq!(status.entries[0].index_status, 'A');

    // Commit → clean tree, hash recorded.
    let hash = repo_handle
        .commit("add hello", false)
        .map_err(|e| e.to_string())?;
    assert!(!hash.is_empty());
    let status = repo_handle.status(false).map_err(|e| e.to_string())?;
    assert!(status.is_clean());
    Ok(())
}

#[test]
fn unstage_keeps_working_tree_file() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("unstage")?;
    repo.write("f.txt", "v1\n")?;
    repo.commit_all("first")?;

    repo.write("f.txt", "v2\n")?;
    let repo_handle = repo.repo();
    let path = repo.root.join("f.txt");
    repo_handle.stage(&[&path]).map_err(|e| e.to_string())?;
    repo_handle.unstage(&[&path]).map_err(|e| e.to_string())?;
    let status = repo_handle.status(false).map_err(|e| e.to_string())?;
    assert_eq!(status.entries.len(), 1);
    assert_eq!(status.entries[0].index_status, '.');
    assert_eq!(status.entries[0].worktree_status, 'M');
    Ok(())
}

#[test]
fn discard_restores_tracked_file_but_never_touches_untracked() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("discard")?;
    repo.write("tracked.txt", "original\n")?;
    repo.commit_all("first")?;

    // Modify tracked, create untracked.
    repo.write("tracked.txt", "changed\n")?;
    repo.write("untracked.txt", "keep me\n")?;
    let repo_handle = repo.repo();
    let tracked = repo.root.join("tracked.txt");
    repo_handle
        .discard(&[&tracked])
        .map_err(|e| e.to_string())?;
    assert_eq!(
        std::fs::read_to_string(repo.root.join("tracked.txt"))
            .unwrap()
            .trim_end_matches(['\r', '\n']),
        "original"
    );
    // Untracked file untouched (checkout -- never affects untracked).
    assert_eq!(
        std::fs::read_to_string(repo.root.join("untracked.txt"))
            .unwrap()
            .trim_end_matches(['\r', '\n']),
        "keep me"
    );
    Ok(())
}

#[test]
fn status_reports_conflict_entries_after_diverged_merge() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("conflict")?;
    repo.write("file.txt", "base\n")?;
    repo.commit_all("base")?;
    run_git(&repo.root, &["checkout", "-b", "feature"])?;
    repo.write("file.txt", "feature line\n")?;
    repo.commit_all("feature change")?;
    run_git(&repo.root, &["checkout", "main"])?;
    repo.write("file.txt", "main line\n")?;
    repo.commit_all("main change")?;
    // Merge attempt leaves conflict markers; git reports unmerged paths.
    let _ = run_git(&repo.root, &["merge", "feature"]);

    let status = repo.repo().status(false).map_err(|e| e.to_string())?;
    assert!(
        !status.conflicts.is_empty(),
        "expected unmerged paths, got: {status:?}"
    );
    assert!(status.conflicts.iter().any(|p| p == "file.txt"));
    // Conflict rows also appear as entries with U status.
    assert!(
        status
            .entries
            .iter()
            .any(|e| e.path == "file.txt" && (e.index_status == 'U' || e.worktree_status == 'U'))
    );
    Ok(())
}

#[test]
fn diff_shows_unified_output_with_content() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("diff")?;
    repo.write("code.txt", "line one\nline two\n")?;
    repo.commit_all("first")?;
    repo.write("code.txt", "line one\nline two changed\n")?;
    let diff = repo
        .repo()
        .diff(DiffFormat::Unstaged, &[])
        .map_err(|e| e.to_string())?;
    assert!(diff.contains("diff --git"));
    assert!(diff.contains("line two changed"));
    // Path bound: diff restricted to another path yields empty.
    let empty = repo
        .repo()
        .diff(DiffFormat::Unstaged, &[Path::new("nonexistent.txt")])
        .map_err(|e| e.to_string())?;
    assert!(empty.trim().is_empty());
    Ok(())
}

#[test]
fn history_lists_commits_newest_first_with_author_fields() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("history")?;
    repo.write("a.txt", "1\n")?;
    repo.commit_all("first commit")?;
    repo.write("b.txt", "2\n")?;
    repo.commit_all("second commit")?;
    repo.write("c.txt", "3\n")?;
    repo.commit_all("third commit")?;

    let commits = repo.repo().history(2).map_err(|e| e.to_string())?;
    assert_eq!(commits.len(), 2, "max_count bound respected");
    assert_eq!(commits[0].subject, "third commit");
    assert_eq!(commits[1].subject, "second commit");
    assert_eq!(commits[0].author_email, "test@example.com");
    assert!(!commits[0].hash.is_empty());
    assert!(!commits[0].abbrev_hash.is_empty());
    Ok(())
}

#[test]
fn discovery_finds_nested_repo_and_rejects_outside_containment() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("discover")?;
    let nested = repo.root.join("src").join("deep");
    std::fs::create_dir_all(&nested).map_err(|e| e.to_string())?;
    let discovered = Repo::discover(&nested).map_err(|e| e.to_string())?;
    assert_eq!(discovered.root, repo.root);
    assert!(discovered.contains(&nested));
    assert!(discovered.contains(&repo.root.join("src").join("x.txt")));
    Ok(())
}

#[test]
fn discovery_rejects_non_repository() -> Result<(), String> {
    require_git!();
    let dir = TempDir::new().map_err(|e| e.to_string())?;
    let result = Repo::discover(dir.path());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "git_not_a_repository");
    Ok(())
}

#[test]
fn discovery_follows_gitdir_file_in_linked_worktree() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("linked")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let wt_path = repo._dir.path().join("wt-linked");
    run_git(
        &repo.root,
        &[
            "worktree",
            "add",
            wt_path.to_str().unwrap(),
            "-b",
            "linked-branch",
        ],
    )?;
    let discovered = Repo::discover(&wt_path).map_err(|e| e.to_string())?;
    // The .git FILE points at the main repo's worktree admin dir.
    assert!(
        wt_path.join(".git").is_file(),
        "linked worktree has a .git FILE"
    );
    assert!(discovered.gitdir.starts_with(repo.root.join(".git")));
    Ok(())
}

#[test]
fn branch_compare_counts_ahead_and_behind() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("compare")?;
    repo.write("base.txt", "base\n")?;
    repo.commit_all("base")?;
    run_git(&repo.root, &["checkout", "-b", "feature"])?;
    repo.write("f1.txt", "f1\n")?;
    repo.commit_all("feature 1")?;
    repo.write("f2.txt", "f2\n")?;
    repo.commit_all("feature 2")?;
    run_git(&repo.root, &["checkout", "main"])?;
    repo.write("m1.txt", "m1\n")?;
    repo.commit_all("main 1")?;

    let (ahead, behind) = repo
        .repo()
        .branch_compare("main", "feature")
        .map_err(|e| e.to_string())?;
    // Feature has 2 commits main lacks; main has 1 commit feature lacks.
    assert_eq!(ahead, 2, "feature is 2 ahead of main");
    assert_eq!(behind, 1, "feature is 1 behind main");
    Ok(())
}

#[test]
fn check_ignored_identifies_gitignored_paths() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("ignored")?;
    repo.write(".gitignore", "secrets/\n*.log\n")?;
    repo.commit_all("gitignore")?;
    repo.write("secrets/key.txt", "hidden\n")?;
    repo.write("debug.log", "noise\n")?;
    repo.write("kept.txt", "visible\n")?;

    let ignored = repo
        .repo()
        .check_ignored(&[
            Path::new("secrets/key.txt"),
            Path::new("debug.log"),
            Path::new("kept.txt"),
        ])
        .map_err(|e| e.to_string())?;
    let texts: Vec<String> = ignored
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    assert!(texts.contains(&"secrets/key.txt".to_string()), "{texts:?}");
    assert!(texts.contains(&"debug.log".to_string()), "{texts:?}");
    assert!(!texts.contains(&"kept.txt".to_string()));
    Ok(())
}

#[test]
fn status_flags_upstream_ahead_behind_after_push_setup() -> Result<(), String> {
    require_git!();
    // Create origin as a bare repo, push, then move main past origin/main.
    let repo = TestRepo::new("upstream")?;
    repo.write("initial.txt", "x\n")?;
    repo.commit_all("initial")?;
    let remote_dir = repo._dir.path().join("remote.git");
    run_git(
        &repo.root,
        &["init", "--bare", remote_dir.to_str().unwrap()],
    )?;
    run_git(
        &repo.root,
        &["remote", "add", "origin", remote_dir.to_str().unwrap()],
    )?;
    run_git(&repo.root, &["push", "origin", "main:main"])?;
    // Advance two commits past origin/main, then set upstream tracking.
    repo.write("a.txt", "a\n")?;
    repo.commit_all("second")?;
    repo.write("b.txt", "b\n")?;
    repo.commit_all("third")?;
    run_git(&repo.root, &["fetch", "origin"])?;
    run_git(&repo.root, &["branch", "--set-upstream-to=origin/main"])?;
    let status = repo.repo().status(false).map_err(|e| e.to_string())?;
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!(status.ahead, Some(2));
    assert_eq!(status.behind, Some(0));
    Ok(())
}

#[test]
fn worktree_create_list_remove_lifecycle() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-lifecycle")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);

    let wt_path = repo._dir.path().join("wt-one");
    let created: CreatedWorktree = service
        .create(&wt_path, Some("wt-branch"), None)
        .map_err(|e| e.to_string())?;
    assert_eq!(created.branch, "wt-branch");
    assert!(!created.head.is_empty());
    assert!(wt_path.join("seed.txt").is_file(), "base content present");

    // List sees it.
    let list = service.list().map_err(|e| e.to_string())?;
    assert!(
        list.iter()
            .any(|e| e.branch.as_deref() == Some("refs/heads/wt-branch"))
    );

    // Clean worktree → removal succeeds.
    service
        .remove(&wt_path, RemoveForce::Never)
        .map_err(|e| e.to_string())?;
    let list = service.list().map_err(|e| e.to_string())?;
    assert!(
        !list
            .iter()
            .any(|e| e.branch.as_deref() == Some("refs/heads/wt-branch"))
    );
    Ok(())
}

#[test]
fn worktree_remove_refuses_dirty_worktree_without_force() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-dirty")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);
    let wt_path = repo._dir.path().join("wt-dirty-one");
    service
        .create(&wt_path, Some("dirty-branch"), None)
        .map_err(|e| e.to_string())?;

    // Dirty it.
    std::fs::write(wt_path.join("seed.txt"), "dirtied\n").map_err(|e| e.to_string())?;
    let err = service.remove(&wt_path, RemoveForce::Never);
    assert!(err.is_err(), "clean removal must refuse dirty worktree");
    assert!(wt_path.exists(), "worktree must survive refusal");

    // Force removes it.
    service
        .remove(&wt_path, RemoveForce::Yes)
        .map_err(|e| e.to_string())?;
    let list = service.list().map_err(|e| e.to_string())?;
    assert!(
        !list
            .iter()
            .any(|e| e.branch.as_deref() == Some("refs/heads/dirty-branch"))
    );
    Ok(())
}

#[test]
fn worktree_remove_refuses_main_worktree_and_bare() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-main-guard")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);

    // Main worktree removal refused.
    let err = service.remove(&repo.root, RemoveForce::Never);
    assert!(err.is_err(), "main worktree removal must be refused");
    let msg = err.unwrap_err().to_string();
    assert!(msg.contains("main worktree"), "{msg}");

    // Bare repo removal refused.
    let bare_dir = repo._dir.path().join("bare.git");
    run_git(&repo.root, &["init", "--bare", bare_dir.to_str().unwrap()])?;
    let bare_repo = Repo::discover(&bare_dir).map_err(|e| e.to_string())?;
    let bare_service = WorktreeService::new(&bare_repo);
    let err = bare_service.remove(&bare_dir, RemoveForce::Never);
    assert!(err.is_err(), "bare removal must be refused");
    Ok(())
}

#[test]
fn worktree_prune_reports_stale_entries() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-prune")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);

    let wt_path = repo._dir.path().join("wt-doomed");
    service
        .create(&wt_path, Some("doomed-branch"), None)
        .map_err(|e| e.to_string())?;
    // Delete the directory out from under git → prunable.
    std::fs::remove_dir_all(&wt_path).map_err(|e| e.to_string())?;

    // Orphan gitdir proof surfaces the path.
    let orphans = service.orphan_gitdirs().map_err(|e| e.to_string())?;
    let wt_text = wt_path.to_string_lossy().replace('\\', "/");
    assert!(
        orphans.iter().any(|p| p.replace('\\', "/") == wt_text),
        "expected orphan listing to contain {wt_text}, got {orphans:?}"
    );

    let report = service.prune().map_err(|e| e.to_string())?;
    assert!(report.is_some(), "prune should report removed entries");
    let orphans_after = service.orphan_gitdirs().map_err(|e| e.to_string())?;
    assert!(
        !orphans_after
            .iter()
            .any(|p| p.replace('\\', "/") == wt_text)
    );
    Ok(())
}

#[test]
fn worktree_lock_unlock_roundtrip() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-lock")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);
    let wt_path = repo._dir.path().join("wt-locked");
    service
        .create(&wt_path, Some("lock-branch"), None)
        .map_err(|e| e.to_string())?;

    service
        .lock(&wt_path, Some("under maintenance"))
        .map_err(|e| e.to_string())?;
    let list = service.list().map_err(|e| e.to_string())?;
    let entry = list
        .iter()
        .find(|e| e.branch.as_deref() == Some("refs/heads/lock-branch"))
        .expect("locked worktree listed");
    assert_eq!(entry.locked_reason.as_deref(), Some("under maintenance"));

    // Locked → removal refused without force.
    assert!(service.remove(&wt_path, RemoveForce::Never).is_err());

    service.unlock(&wt_path).map_err(|e| e.to_string())?;
    let list = service.list().map_err(|e| e.to_string())?;
    let entry = list
        .iter()
        .find(|e| e.branch.as_deref() == Some("refs/heads/lock-branch"))
        .expect("unlocked worktree listed");
    assert!(entry.locked_reason.is_none());
    Ok(())
}

#[test]
fn worktree_repair_is_idempotent_on_healthy_repo() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-repair")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);
    service.repair().map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn worktree_create_rejects_nonempty_existing_path() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("wt-occupied")?;
    repo.write("seed.txt", "seed\n")?;
    repo.commit_all("seed")?;
    let repo_handle = repo.repo();
    let service = WorktreeService::new(&repo_handle);
    let wt_path = repo._dir.path().join("wt-occupied-target");
    std::fs::create_dir_all(&wt_path).map_err(|e| e.to_string())?;
    std::fs::write(wt_path.join("occupied.txt"), "occupied\n").map_err(|e| e.to_string())?;
    let err = service.create(&wt_path, Some("occupied-branch"), None);
    assert!(err.is_err(), "must refuse existing non-empty path");
    Ok(())
}

#[test]
fn submodule_status_reports_uninitialized_submodule() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("submodule-host")?;
    repo.write("host.txt", "host\n")?;
    repo.commit_all("host")?;
    // Create a genuine child repo to add as submodule.
    let child = TestRepo::new("submodule-child")?;
    child.write("child.txt", "child\n")?;
    child.commit_all("child")?;
    let added = run_git(
        &repo.root,
        &[
            "submodule",
            "add",
            child.root.to_str().unwrap(),
            "libs/child",
        ],
    );
    if added.is_err() {
        // Some git configs refuse local submodule adds (safe.directory etc.).
        eprintln!("WARNING: submodule add unavailable on this host; partial skip");
        return Ok(());
    }
    let status = repo
        .repo()
        .submodule_status(Path::new("libs/child"))
        .map_err(|e| e.to_string())?;
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].author_name, "libs/child");
    assert!(!status[0].hash.is_empty());
    Ok(())
}

#[test]
fn commit_respects_allow_empty_flag() -> Result<(), String> {
    require_git!();
    let repo = TestRepo::new("empty-commit")?;
    repo.write("x.txt", "x\n")?;
    repo.commit_all("first")?;
    let repo_handle = repo.repo();
    // Without allow-empty this fails; with it, a new commit is produced.
    let head_before = run_git(&repo.root, &["rev-parse", "HEAD"])?;
    repo_handle
        .commit("empty one", true)
        .map_err(|e| e.to_string())?;
    let head_after = run_git(&repo.root, &["rev-parse", "HEAD"])?;
    assert_ne!(head_before.trim(), head_after.trim());
    Ok(())
}

#[test]
fn runner_rejects_non_repository_cwd_with_typed_error() -> Result<(), String> {
    require_git!();
    let dir = TempDir::new().map_err(|e| e.to_string())?;
    let runner = GitRunner::shared().map_err(|e| e.to_string())?;
    let err = runner.run(
        dir.path(),
        std::time::Duration::from_secs(15),
        &[OsStr::new("status")],
    );
    assert_eq!(err.unwrap_err().code(), "git_not_a_repository");
    Ok(())
}

// Windows long-path handling (cfg-gated).
#[cfg(windows)]
#[test]
fn long_path_file_survives_stage_commit_on_windows() -> Result<(), String> {
    use mcode_git::maybe_long_path;
    require_git!();
    let repo = TestRepo::new("longpath")?;
    run_git(&repo.root, &["config", "core.longpaths", "true"])?;
    let mut deep = repo.root.clone();
    let mut total = deep.as_os_str().len();
    let mut i = 0;
    while total < 280 {
        let seg = format!("segment-{i:02}");
        deep.push(&seg);
        total += seg.len() + 1;
        i += 1;
    }
    std::fs::create_dir_all(&deep).map_err(|e| e.to_string())?;
    let file = deep.join("leaf.txt");
    std::fs::write(&file, "deep content\n").map_err(|e| e.to_string())?;
    assert!(file.as_os_str().len() > 260);
    let repo_handle = repo.repo();
    let deep_file = maybe_long_path(&file);
    repo_handle
        .stage(&[&deep_file])
        .map_err(|e| e.to_string())?;
    let status = repo_handle.status(false).map_err(|e| e.to_string())?;
    assert_eq!(status.entries.len(), 1, "deep file staged");
    Ok(())
}

// ---------------------------------------------------------------------------
// WSL path translation pure tests (host-independent).
// ---------------------------------------------------------------------------

#[test]
fn wsl_translation_pure_roundtrips() {
    use mcode_git::{to_wsl_text, windows_to_wsl, wsl_to_windows};
    // Windows -> WSL.
    assert_eq!(
        windows_to_wsl(Path::new(r"C:\Users\me\repo"))
            .unwrap()
            .linux,
        "/mnt/c/Users/me/repo"
    );
    // WSL -> Windows.
    assert_eq!(
        wsl_to_windows("/mnt/c/Users/me/repo").unwrap(),
        PathBuf::from(r"C:\Users\me\repo")
    );
    // Non-mnt POSIX paths stay on the Linux side.
    assert_eq!(to_wsl_text(Path::new("/home/u/repo")), "/home/u/repo");
    assert!(wsl_to_windows("/home/u/repo").is_none());
}
